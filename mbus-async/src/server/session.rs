//! [`AsyncServerSession`] — the async Modbus server dispatch loop.
//!
//! A session wraps a single transport connection. Pass any `impl AsyncAppHandler` to
//! [`run`](AsyncServerSession::run) and the session handles framing, FC08 sub-functions,
//! listen-only mode, broadcast filtering, and statistics internally.

use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, Pdu, decompile_adu_frame};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{AsyncTransport, TransportType, UnitIdOrSlaveAddr};

#[cfg(feature = "file-record")]
use super::app_handler::AsyncFileRecordWriteSubRequest;
use super::app_handler::{AsyncAppHandler, AsyncServerError, ModbusRequest, ModbusResponse};
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::MAX_SUB_REQUESTS_PER_PDU;

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;

#[cfg(feature = "diagnostics-stats")]
use super::statistics::AsyncServerStatistics;

// ── Logging macros ───────────────────────────────────────────────────────────

#[cfg(feature = "logging")]
macro_rules! async_log_debug {
    ($($arg:tt)*) => { log::debug!($($arg)*) };
}
#[cfg(not(feature = "logging"))]
macro_rules! async_log_debug {
    ($($arg:tt)*) => {{ let _ = core::format_args!($($arg)*); }};
}

// ── AsyncServerSession ───────────────────────────────────────────────────────

/// An async Modbus server session over a single transport connection.
///
/// Generic over any type implementing [`AsyncTransport`]. For TCP, obtain sessions
/// via [`AsyncTcpServer::accept`](super::tcp_server::AsyncTcpServer::accept).
/// For serial, use [`AsyncSerialServer`](super::serial_server::AsyncSerialServer) directly.
pub struct AsyncServerSession<T: AsyncTransport + Send> {
    transport: T,
    unit: UnitIdOrSlaveAddr,

    /// When `true` the server is in "Listen Only Mode" (FC08/0x0004).
    ///
    /// All incoming requests except FC08 (Diagnostics) are silently discarded — the
    /// application handler is never called and no response is sent.  The mode is cleared
    /// automatically when FC08/0x0001 (Restart Communications Option) is received.
    ///
    /// Level 1 (`run`) enforces this automatically.  Level 2 callers must honour
    /// [`listen_only_mode()`](Self::listen_only_mode) themselves, or configure the
    /// session via [`set_listen_only_mode()`](Self::set_listen_only_mode).
    #[cfg(feature = "diagnostics")]
    listen_only_mode: bool,

    /// When `true`, broadcast write requests (unit address 0x00) received over a
    /// serial transport are dispatched to the application but **no response is sent**,
    /// matching the Modbus spec for serial broadcast semantics.
    ///
    /// Only meaningful when using a serial transport; TCP sessions should leave this
    /// at its default `false`.  Non-write broadcast frames are silently discarded
    /// regardless of this flag.
    enable_broadcast_writes: bool,

    /// Protocol-level statistics counters (FC08 Diagnostics sub-function responses).
    /// Only present when the `diagnostics-stats` feature is enabled.
    #[cfg(feature = "diagnostics-stats")]
    stats: AsyncServerStatistics,
}

impl<T: AsyncTransport + Send> AsyncServerSession<T> {
    /// Create a new session wrapping the given transport.
    pub fn new(transport: T, unit: UnitIdOrSlaveAddr) -> Self {
        Self {
            transport,
            unit,
            #[cfg(feature = "diagnostics")]
            listen_only_mode: false,
            enable_broadcast_writes: false,
            #[cfg(feature = "diagnostics-stats")]
            stats: AsyncServerStatistics::new(),
        }
    }

    /// Whether the underlying transport is currently connected.
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    // ── Configuration ────────────────────────────────────────────────────────

    /// Returns whether the session is currently in Listen Only Mode.
    ///
    /// In this mode, Level 1 (`run`) silently drops all requests except FC08 Diagnostics.
    /// Level 2 callers should check this before deciding whether to respond.
    #[cfg(feature = "diagnostics")]
    pub fn listen_only_mode(&self) -> bool {
        self.listen_only_mode
    }

    /// Manually override the Listen Only Mode flag.
    ///
    /// Normally this is managed automatically by FC08 sub-function 0x0004 (set) and
    /// 0x0001 (clear).  Use this to pre-configure the session or to bypass the flag
    /// from Level 2 application code.
    #[cfg(feature = "diagnostics")]
    pub fn set_listen_only_mode(&mut self, enabled: bool) {
        self.listen_only_mode = enabled;
    }

    /// Returns whether serial broadcast write suppression is enabled.
    pub fn broadcast_writes_enabled(&self) -> bool {
        self.enable_broadcast_writes
    }

    /// Enable or disable serial broadcast write suppression.
    ///
    /// When `true`, write requests addressed to unit 0x00 (broadcast) are dispatched to
    /// the application handler but **no response is sent**, matching Modbus serial
    /// broadcast semantics.  Non-write broadcast frames are silently discarded.
    ///
    /// Defaults to `false`.  Only meaningful over serial transports.
    pub fn set_broadcast_writes(&mut self, enabled: bool) {
        self.enable_broadcast_writes = enabled;
    }

    /// Returns a snapshot of the protocol-level statistics counters.
    #[cfg(feature = "diagnostics-stats")]
    pub fn stats(&self) -> &AsyncServerStatistics {
        &self.stats
    }

    /// Returns a mutable reference to the statistics counters (e.g. to clear them).
    #[cfg(feature = "diagnostics-stats")]
    pub fn stats_mut(&mut self) -> &mut AsyncServerStatistics {
        &mut self.stats
    }

    /// Run the server session loop until the connection is lost.
    ///
    /// This method handles the full request/response cycle automatically:
    ///
    /// - **Listen Only Mode**: non-FC08 requests are silently dropped while the flag is set.
    /// - **FC08 loopback / restart / listen-only sub-functions (0x0000, 0x0001, 0x0004)**:
    ///   echoed or suppressed by the session without calling the application handler.
    /// - **FC08 counter sub-functions (0x000B–0x0011, 0x000A, 0x0014)** (`diagnostics-stats`
    ///   feature): answered directly from internal statistics counters.
    /// - **Broadcast write suppression**: when [`set_broadcast_writes(true)`](Self::set_broadcast_writes)
    ///   is set, write requests addressed to unit 0x00 are dispatched to the application
    ///   but no response is sent.
    ///
    /// Returns `Err(AsyncServerError::ConnectionClosed)` when the client disconnects.
    pub async fn run<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
    ) -> Result<(), AsyncServerError> {
        loop {
            let Some(r) = self.recv_request(app).await? else { continue };

            #[cfg(feature = "diagnostics")]
            if self.should_discard_in_listen_only_mode(&r.req, r.txn_id) {
                continue;
            }

            #[cfg(feature = "diagnostics")]
            if self.try_dispatch_fc08_auto(r.txn_id, r.unit, &r.req).await? {
                continue;
            }

            if r.unit.is_broadcast() && r.transport_type.is_serial_type() {
                self.handle_serial_broadcast(app, r.req, r.txn_id, r.unit).await;
                continue;
            }

            if self.try_reply_unknown_fc(app, &r.req, r.txn_id, r.unit).await? {
                continue;
            }

            self.dispatch_and_send(app, &r.adu, r.req, r.txn_id, r.unit).await?;
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Receive one ADU from the transport, parse it, and handle framing errors.
    ///
    /// Returns `Ok(None)` for both wrong-unit frames and framing errors (caller
    /// should `continue` to the next loop iteration). Returns `Err` only for
    /// fatal transport errors.
    async fn recv_request<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
    ) -> Result<Option<ReceivedRequest>, AsyncServerError> {
        let adu = self.transport.recv().await.map_err(AsyncServerError::from)?;

        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_message_count();

        let transport_type = self.transport.transport_type();
        match self.parse_adu(&adu, transport_type) {
            Ok(Some(req)) => {
                let txn_id = req.txn_id();
                let unit = req.unit();
                Ok(Some(ReceivedRequest { adu, req, txn_id, unit, transport_type }))
            }
            Ok(None) => Ok(None), // wrong unit address — discard silently
            Err(AsyncServerError::FramingError(e)) => {
                #[cfg(feature = "diagnostics-stats")]
                self.stats.increment_comm_error_count();
                #[cfg(feature = "traffic")]
                app.on_rx_error(0, self.unit, e, &adu);
                async_log_debug!("framing error on received ADU, discarding: {:?}", e);
                Ok(None) // signal "continue"
            }
            Err(e) => Err(e),
        }
    }

    /// Returns `true` if the request must be silently dropped because the session
    /// is in Listen Only Mode and the FC is not FC08 (Diagnostics).
    /// Also increments the no-response counter when discarding.
    #[cfg(feature = "diagnostics")]
    fn should_discard_in_listen_only_mode(&mut self, req: &ModbusRequest, txn_id: u16) -> bool {
        if self.listen_only_mode && !matches!(req, ModbusRequest::Diagnostics { .. }) {
            async_log_debug!(
                "listen-only: discarding fc=0x{:02X} txn_id={}",
                req.function_code_byte(),
                txn_id
            );
            #[cfg(feature = "diagnostics-stats")]
            self.stats.increment_no_response_count();
            return true;
        }
        false
    }

    /// Checks if `req` is an FC08 Diagnostics request and, if so, delegates the
    /// sub-function to [`handle_fc08_auto`](Self::handle_fc08_auto).
    ///
    /// Returns `Ok(true)` when the sub-function was fully handled (caller should
    /// `continue`), `Ok(false)` if the request should be forwarded to the app.
    #[cfg(feature = "diagnostics")]
    async fn try_dispatch_fc08_auto(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        req: &ModbusRequest,
    ) -> Result<bool, AsyncServerError> {
        if let ModbusRequest::Diagnostics { sub_function, data, .. } = req {
            return self.handle_fc08_auto(txn_id, unit, *sub_function, *data).await;
        }
        Ok(false)
    }

    /// Handles a serial broadcast frame: dispatches write FCs to the app without
    /// sending a response, and silently discards everything else.
    async fn handle_serial_broadcast<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
        req: ModbusRequest,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
    ) {
        if self.enable_broadcast_writes && is_broadcast_write_fc(&req) {
            async_log_debug!(
                "broadcast write fc=0x{:02X} txn_id={}: dispatching to app, no response",
                req.function_code_byte(),
                txn_id
            );
            #[cfg(feature = "diagnostics-stats")]
            self.stats.increment_server_message_count();
            app.handle(req).await;
        } else {
            async_log_debug!(
                "serial broadcast discarded fc=0x{:02X} txn_id={} (broadcast_writes={})",
                req.function_code_byte(),
                txn_id,
                self.enable_broadcast_writes
            );
        }
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_no_response_count();
    }

    /// If `req` is an `Unknown` FC, sends an `IllegalFunction` exception and returns
    /// `Ok(true)` (caller should `continue`). Returns `Ok(false)` for all other variants.
    async fn try_reply_unknown_fc<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
        req: &ModbusRequest,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<bool, AsyncServerError> {
        if let ModbusRequest::Unknown { function_code: fc_byte, .. } = req {
            let fc_byte = *fc_byte;
            async_log_debug!(
                "unknown/disabled FC=0x{:02X} txn_id={}: replying IllegalFunction",
                fc_byte,
                txn_id
            );
            #[cfg(feature = "diagnostics-stats")]
            self.stats.increment_exception_error_count();
            if let Ok(fc) = FunctionCode::try_from(fc_byte) {
                let resp = ModbusResponse::exception(fc, ExceptionCode::IllegalFunction);
                app.on_exception(txn_id, unit, fc, ExceptionCode::IllegalFunction);
                self.respond(txn_id, unit, resp).await?;
            }
            return Ok(true);
        }
        Ok(false)
    }

    /// Dispatches a known request to the app, updates statistics, fires traffic
    /// hooks, and sends the encoded response back to the client.
    async fn dispatch_and_send<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
        adu: &[u8],
        req: ModbusRequest,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<(), AsyncServerError> {
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_server_message_count();

        #[cfg(feature = "traffic")]
        app.on_rx_frame(txn_id, unit, adu);

        let resp = app.handle(req).await;

        self.update_response_stats(&resp);
        self.notify_exception(app, txn_id, unit, &resp);
        self.send_response(app, txn_id, unit, resp).await
    }

    /// Updates diagnostics-stats counters based on the response type.
    fn update_response_stats(&mut self, resp: &ModbusResponse) {
        #[cfg(feature = "diagnostics-stats")]
        match resp {
            ModbusResponse::Exception { .. } => self.stats.increment_exception_error_count(),
            ModbusResponse::NoResponse => self.stats.increment_no_response_count(),
            _ => {}
        }
    }

    /// Calls `app.on_exception` when the response is an exception frame.
    fn notify_exception<APP: AsyncAppHandler>(
        &self,
        app: &mut APP,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        resp: &ModbusResponse,
    ) {
        if let ModbusResponse::Exception { request_fc, code } = resp {
            app.on_exception(txn_id, unit, *request_fc, *code);
        }
    }

    /// Encodes `resp` and writes it to the transport, firing traffic hooks on
    /// success or error. Skips silently for `NoResponse`.
    async fn send_response<APP: AsyncAppHandler>(
        &mut self,
        app: &mut APP,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        resp: ModbusResponse,
    ) -> Result<(), AsyncServerError> {
        if matches!(resp, ModbusResponse::NoResponse) {
            return Ok(());
        }
        let tt = self.transport.transport_type();
        let frame = resp.encode(txn_id, unit, tt).map_err(AsyncServerError::Transport)?;
        match self.transport.send(&frame).await {
            Ok(_) => {
                #[cfg(feature = "traffic")]
                app.on_tx_frame(txn_id, unit, &frame);
                Ok(())
            }
            Err(e) => {
                #[cfg(feature = "traffic")]
                app.on_tx_error(txn_id, unit, MbusError::SendFailed, &frame);
                Err(AsyncServerError::from(e))
            }
        }
    }

    async fn respond(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        resp: ModbusResponse,
    ) -> Result<(), AsyncServerError> {
        if matches!(resp, ModbusResponse::NoResponse) {
            return Ok(());
        }
        let transport_type = self.transport.transport_type();
        let adu = resp
            .encode(txn_id, unit, transport_type)
            .map_err(AsyncServerError::Transport)?;
        self.transport
            .send(&adu)
            .await
            .map_err(AsyncServerError::from)
    }

    /// Handle FC08 sub-functions that are managed entirely by the session, without
    /// forwarding to the application handler.
    ///
    /// Returns `Ok(true)` when the sub-function was handled (caller should `continue`).
    /// Returns `Ok(false)` when the sub-function should be forwarded to the app.
    #[cfg(feature = "diagnostics")]
    async fn handle_fc08_auto(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
        data: u16,
    ) -> Result<bool, AsyncServerError> {
        match sub_fn {
            x if x == DiagnosticSubFunction::ReturnQueryData as u16 => {
                self.fc08_echo_query(txn_id, unit, sub_fn, data).await
            }
            x if x == DiagnosticSubFunction::RestartCommunicationsOption as u16 => {
                self.fc08_restart_comms(txn_id, unit, sub_fn, data).await
            }
            x if x == DiagnosticSubFunction::ForceListenOnlyMode as u16 => {
                self.fc08_force_listen_only(txn_id).await
            }
            #[cfg(feature = "diagnostics-stats")]
            _ => Ok(self.handle_stats_sub_fn(txn_id, unit, sub_fn).await),
            #[cfg(not(feature = "diagnostics-stats"))]
            _ => Ok(false),
        }
    }

    /// FC08/0x0000 — Return Query Data: echo sub_fn + data back unchanged.
    #[cfg(feature = "diagnostics")]
    async fn fc08_echo_query(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
        data: u16,
    ) -> Result<bool, AsyncServerError> {
        async_log_debug!("FC08/0x0000: loopback echo; txn_id={}", txn_id);
        let resp = ModbusResponse::diagnostics_echo(sub_fn, data);
        self.respond(txn_id, unit, resp).await?;
        Ok(true)
    }

    /// FC08/0x0001 — Restart Communications Option: clear listen-only mode and echo.
    #[cfg(feature = "diagnostics")]
    async fn fc08_restart_comms(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
        data: u16,
    ) -> Result<bool, AsyncServerError> {
        self.listen_only_mode = false;
        async_log_debug!("FC08/0x0001: listen-only mode cleared; txn_id={}", txn_id);
        let resp = ModbusResponse::diagnostics_echo(sub_fn, data);
        self.respond(txn_id, unit, resp).await?;
        Ok(true)
    }

    /// FC08/0x0004 — Force Listen Only Mode: enable; send no response per spec.
    #[cfg(feature = "diagnostics")]
    async fn fc08_force_listen_only(&mut self, txn_id: u16) -> Result<bool, AsyncServerError> {
        self.listen_only_mode = true;
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_no_response_count();
        async_log_debug!("FC08/0x0004: listen-only mode enabled; txn_id={}", txn_id);
        Ok(true)
    }

    /// Handle FC08 statistics counter sub-functions (only when `diagnostics-stats` is enabled).
    ///
    /// Returns `true` if the sub-function was handled, `false` to fall through to the app.
    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    async fn handle_stats_sub_fn(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
    ) -> bool {
        // Mutating sub-functions are handled first (they reset state before echoing).
        match DiagnosticSubFunction::try_from(sub_fn) {
            Ok(DiagnosticSubFunction::ClearCountersAndDiagnosticRegister) => {
                return self.fc08_clear_all_counters(txn_id, unit, sub_fn).await;
            }
            Ok(DiagnosticSubFunction::ClearOverrunCounterAndFlag) => {
                return self.fc08_clear_overrun(txn_id, unit, sub_fn).await;
            }
            _ => {}
        }

        // Read-only counter sub-functions.
        match self.resolve_stats_counter(sub_fn) {
            Some(counter) => {
                let resp = ModbusResponse::diagnostics_echo(sub_fn, counter);
                let _ = self.respond(txn_id, unit, resp).await;
                true
            }
            None => false,
        }
    }

    /// FC08/0x000A — Clear all counters and diagnostic register, then echo.
    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    async fn fc08_clear_all_counters(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
    ) -> bool {
        self.stats.clear();
        let resp = ModbusResponse::diagnostics_echo(sub_fn, 0);
        let _ = self.respond(txn_id, unit, resp).await;
        true
    }

    /// FC08/0x0014 — Clear overrun counter and flag, then echo.
    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    async fn fc08_clear_overrun(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        sub_fn: u16,
    ) -> bool {
        // No hardware overrun concept in async; clear the counter and echo.
        self.stats.clear_overrun();
        let resp = ModbusResponse::diagnostics_echo(sub_fn, 0);
        let _ = self.respond(txn_id, unit, resp).await;
        true
    }

    /// Maps a read-only FC08 counter sub-function to a `u16` counter value.
    /// Returns `None` for unknown sub-functions.
    #[cfg(all(feature = "diagnostics", feature = "diagnostics-stats"))]
    fn resolve_stats_counter(&self, sub_fn: u16) -> Option<u16> {
        match DiagnosticSubFunction::try_from(sub_fn) {
            Ok(DiagnosticSubFunction::ReturnBusMessageCount) => Some(self.stats.message_count),
            Ok(DiagnosticSubFunction::ReturnBusCommunicationErrorCount) => {
                Some(self.stats.comm_error_count)
            }
            Ok(DiagnosticSubFunction::ReturnBusExceptionErrorCount) => {
                Some(self.stats.exception_error_count)
            }
            Ok(DiagnosticSubFunction::ReturnServerMessageCount) => {
                Some(self.stats.server_message_count)
            }
            Ok(DiagnosticSubFunction::ReturnServerNoResponseCount) => {
                Some(self.stats.no_response_count)
            }
            Ok(DiagnosticSubFunction::ReturnServerNakCount) => Some(self.stats.nak_count),
            Ok(DiagnosticSubFunction::ReturnServerBusyCount) => Some(self.stats.busy_count),
            Ok(DiagnosticSubFunction::ReturnBusCharacterOverrunCount) => {
                Some(self.stats.character_overrun_count)
            }
            _ => None,
        }
    }

    /// Parse a raw ADU into a typed `ModbusRequest`.
    ///
    /// Returns `Ok(None)` if the frame is addressed to a different unit (drop silently).
    /// Returns `Err(AsyncServerError::FramingError(_))` for malformed frames.
    fn parse_adu(
        &self,
        adu: &[u8],
        transport_type: TransportType,
    ) -> Result<Option<ModbusRequest>, AsyncServerError> {
        let message =
            decompile_adu_frame(adu, transport_type).map_err(AsyncServerError::FramingError)?;

        let unit = message.unit_id_or_slave_addr();
        if !self.is_addressed_to_us(unit) {
            return Ok(None);
        }

        let txn_id = message.transaction_id();
        let pdu = message.pdu();
        let fc = pdu.function_code();

        let req = match fc {
            #[cfg(feature = "coils")]
            FunctionCode::ReadCoils
            | FunctionCode::WriteSingleCoil
            | FunctionCode::WriteMultipleCoils => parse_coil_request(txn_id, unit, fc, pdu)?,

            #[cfg(feature = "discrete-inputs")]
            FunctionCode::ReadDiscreteInputs => parse_discrete_input_request(txn_id, unit, pdu)?,

            #[cfg(feature = "registers")]
            FunctionCode::ReadHoldingRegisters
            | FunctionCode::WriteSingleRegister
            | FunctionCode::WriteMultipleRegisters
            | FunctionCode::ReadInputRegisters
            | FunctionCode::MaskWriteRegister
            | FunctionCode::ReadWriteMultipleRegisters => {
                parse_register_request(txn_id, unit, fc, pdu)?
            }

            #[cfg(feature = "diagnostics")]
            FunctionCode::ReadExceptionStatus
            | FunctionCode::Diagnostics
            | FunctionCode::GetCommEventCounter
            | FunctionCode::GetCommEventLog
            | FunctionCode::ReportServerId
            | FunctionCode::EncapsulatedInterfaceTransport => {
                parse_diagnostics_request(txn_id, unit, fc, pdu)?
            }

            #[cfg(feature = "fifo")]
            FunctionCode::ReadFifoQueue => parse_fifo_request(txn_id, unit, pdu)?,

            #[cfg(feature = "file-record")]
            FunctionCode::ReadFileRecord | FunctionCode::WriteFileRecord => {
                parse_file_record_request(txn_id, unit, fc, pdu)?
            }

            _ => ModbusRequest::Unknown {
                txn_id,
                unit,
                function_code: fc as u8,
            },
        };

        Ok(Some(req))
    }

    /// Returns `true` if the given unit address is this session's unit or is a broadcast.
    fn is_addressed_to_us(&self, unit: UnitIdOrSlaveAddr) -> bool {
        unit == self.unit || unit.is_broadcast()
    }
}

// ── ReceivedRequest ──────────────────────────────────────────────────────────

/// Bundles the result of one receive + parse cycle for use by the `run()` loop.
struct ReceivedRequest {
    /// Raw ADU bytes, needed by the `traffic` feature hooks.
    adu: heapless::Vec<u8, MAX_ADU_FRAME_LEN>,
    req: ModbusRequest,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    transport_type: TransportType,
}

// ── Free helpers ─────────────────────────────────────────────────────────────

/// Returns `true` if `req` is one of the four write FCs that are valid as Modbus
/// serial broadcast commands (per spec: FC05, FC06, FC0F, FC10).
fn is_broadcast_write_fc(req: &ModbusRequest) -> bool {
    matches!(req.function_code_byte(), 0x05 | 0x06 | 0x0F | 0x10)
}

// ── Per-feature PDU parsers ───────────────────────────────────────────────────

#[cfg(feature = "coils")]
fn parse_coil_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    match fc {
        FunctionCode::ReadCoils => {
            let w = pdu.read_window().map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::ReadCoils {
                txn_id,
                unit,
                address: w.address,
                count: w.quantity,
            })
        }
        FunctionCode::WriteSingleCoil => {
            let f = pdu
                .write_single_u16_fields()
                .map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::WriteSingleCoil {
                txn_id,
                unit,
                address: f.address,
                value: f.value == 0xFF00,
            })
        }
        _ => {
            // WriteMultipleCoils
            let f = pdu
                .write_multiple_fields()
                .map_err(AsyncServerError::FramingError)?;
            let mut data: heapless::Vec<u8, MAX_ADU_FRAME_LEN> = heapless::Vec::new();
            data.extend_from_slice(f.values)
                .map_err(|_| AsyncServerError::FramingError(MbusError::BufferTooSmall))?;
            Ok(ModbusRequest::WriteMultipleCoils {
                txn_id,
                unit,
                address: f.address,
                count: f.quantity,
                data,
            })
        }
    }
}

#[cfg(feature = "discrete-inputs")]
fn parse_discrete_input_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    let w = pdu.read_window().map_err(AsyncServerError::FramingError)?;
    Ok(ModbusRequest::ReadDiscreteInputs {
        txn_id,
        unit,
        address: w.address,
        count: w.quantity,
    })
}

#[cfg(feature = "registers")]
fn parse_register_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    match fc {
        FunctionCode::ReadHoldingRegisters => {
            let w = pdu.read_window().map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::ReadHoldingRegisters {
                txn_id,
                unit,
                address: w.address,
                count: w.quantity,
            })
        }
        FunctionCode::WriteSingleRegister => {
            let f = pdu
                .write_single_u16_fields()
                .map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::WriteSingleRegister {
                txn_id,
                unit,
                address: f.address,
                value: f.value,
            })
        }
        FunctionCode::WriteMultipleRegisters => {
            let f = pdu
                .write_multiple_fields()
                .map_err(AsyncServerError::FramingError)?;
            let mut data: heapless::Vec<u8, MAX_ADU_FRAME_LEN> = heapless::Vec::new();
            data.extend_from_slice(f.values)
                .map_err(|_| AsyncServerError::FramingError(MbusError::BufferTooSmall))?;
            Ok(ModbusRequest::WriteMultipleRegisters {
                txn_id,
                unit,
                address: f.address,
                count: f.quantity,
                data,
            })
        }
        FunctionCode::ReadInputRegisters => {
            let w = pdu.read_window().map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::ReadInputRegisters {
                txn_id,
                unit,
                address: w.address,
                count: w.quantity,
            })
        }
        FunctionCode::MaskWriteRegister => {
            let f = pdu
                .mask_write_register_fields()
                .map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::MaskWriteRegister {
                txn_id,
                unit,
                address: f.address,
                and_mask: f.and_mask,
                or_mask: f.or_mask,
            })
        }
        _ => {
            // ReadWriteMultipleRegisters
            let f = pdu
                .read_write_multiple_fields()
                .map_err(AsyncServerError::FramingError)?;
            let mut data: heapless::Vec<u8, MAX_ADU_FRAME_LEN> = heapless::Vec::new();
            data.extend_from_slice(f.write_values)
                .map_err(|_| AsyncServerError::FramingError(MbusError::BufferTooSmall))?;
            Ok(ModbusRequest::ReadWriteMultipleRegisters {
                txn_id,
                unit,
                read_address: f.read_address,
                read_count: f.read_quantity,
                write_address: f.write_address,
                write_count: f.write_quantity,
                data,
            })
        }
    }
}

#[cfg(feature = "diagnostics")]
fn parse_diagnostics_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    match fc {
        FunctionCode::ReadExceptionStatus => {
            Ok(ModbusRequest::ReadExceptionStatus { txn_id, unit })
        }
        FunctionCode::Diagnostics => {
            let (sub_fn, data) = pdu
                .diagnostics_fields()
                .map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::Diagnostics {
                txn_id,
                unit,
                sub_function: sub_fn,
                data,
            })
        }
        FunctionCode::GetCommEventCounter => {
            Ok(ModbusRequest::GetCommEventCounter { txn_id, unit })
        }
        FunctionCode::GetCommEventLog => Ok(ModbusRequest::GetCommEventLog { txn_id, unit }),
        FunctionCode::ReportServerId => Ok(ModbusRequest::ReportServerId { txn_id, unit }),
        _ => {
            // EncapsulatedInterfaceTransport
            let mei = pdu
                .mei_type_payload()
                .map_err(AsyncServerError::FramingError)?;
            let mut data: heapless::Vec<u8, MAX_ADU_FRAME_LEN> = heapless::Vec::new();
            data.extend_from_slice(mei.payload)
                .map_err(|_| AsyncServerError::FramingError(MbusError::BufferTooSmall))?;
            Ok(ModbusRequest::EncapsulatedInterfaceTransport {
                txn_id,
                unit,
                mei_type: mei.mei_type_byte,
                data,
            })
        }
    }
}

#[cfg(feature = "fifo")]
fn parse_fifo_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    let ptr = pdu.fifo_pointer().map_err(AsyncServerError::FramingError)?;
    Ok(ModbusRequest::ReadFifoQueue {
        txn_id,
        unit,
        pointer_address: ptr,
    })
}

#[cfg(feature = "file-record")]
fn parse_file_record_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    pdu: &Pdu,
) -> Result<ModbusRequest, AsyncServerError> {
    match fc {
        FunctionCode::ReadFileRecord => {
            let sub_reqs = pdu
                .file_record_read_sub_requests()
                .map_err(AsyncServerError::FramingError)?;
            Ok(ModbusRequest::ReadFileRecord {
                txn_id,
                unit,
                sub_requests: sub_reqs,
            })
        }
        _ => {
            // WriteFileRecord
            let borrowed = pdu
                .file_record_write_sub_requests()
                .map_err(AsyncServerError::FramingError)?;
            let mut sub_requests: heapless::Vec<
                AsyncFileRecordWriteSubRequest,
                MAX_SUB_REQUESTS_PER_PDU,
            > = heapless::Vec::new();
            for b in &borrowed {
                let mut rd: heapless::Vec<u8, MAX_ADU_FRAME_LEN> = heapless::Vec::new();
                rd.extend_from_slice(b.record_data_bytes)
                    .map_err(|_| AsyncServerError::FramingError(MbusError::BufferTooSmall))?;
                let _ = sub_requests.push(AsyncFileRecordWriteSubRequest {
                    file_number: b.file_number,
                    record_number: b.record_number,
                    record_length: b.record_length,
                    record_data: rd,
                });
            }
            let raw_pdu_data = pdu.data().clone();
            Ok(ModbusRequest::WriteFileRecord {
                txn_id,
                unit,
                sub_requests,
                raw_pdu_data,
            })
        }
    }
}
