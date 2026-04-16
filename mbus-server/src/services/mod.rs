//! # Modbus Server Services
//!
//! This module provides the core orchestration logic for a Modbus server.
//! It manages the transport lifecycle, receives incoming frames, and routes
//! them to application-level handlers.
//!
//! ## Key Components
//! - [`ServerServices`]: The main entry point. Owns the transport and the
//!   application handler. Call `poll()` in a tight loop to process incoming
//!   Modbus requests.
//! - Sub-modules: Specialized modules (register, coils, etc.) that handle the
//!   serialization and deserialization of specific Modbus function codes.
//! - [`exception`]: Centralized exception response handling and encoding.

#[cfg(feature = "coils")]
pub mod coils;
#[cfg(feature = "diagnostics")]
pub mod device_info;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "discrete-inputs")]
pub mod discrete_input;
pub mod exception;
#[cfg(feature = "fifo")]
pub mod fifo;
#[cfg(feature = "file-record")]
pub mod file_record;
pub mod framing;
#[cfg(any(feature = "holding-registers", feature = "input-registers"))]
pub mod register;
pub mod resilience;

use crate::app::ModbusAppHandler;
use heapless::{Deque, Vec};
use mbus_core::{
    data_unit::common::{
        self, AdditionalAddress, MAX_ADU_FRAME_LEN, ModbusMessage, SlaveAddress,
        derive_length_from_bytes,
    },
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{ModbusConfig, Transport, TransportType, UnitIdOrSlaveAddr},
};
use resilience::{
    OverflowPolicy, PendingRequest, PendingResponse, RequestPriority, RequestQueue,
    ResilienceConfig, ResponseQueue,
};

// ---------------------------------------------------------------------------
// Internal logging macros
// ---------------------------------------------------------------------------

#[cfg(feature = "logging")]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {
        log::debug!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! server_log_debug {
    ($($arg:tt)*) => {{
        let _ = core::format_args!($($arg)*);
    }};
}

#[cfg(feature = "logging")]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {
        log::trace!($($arg)*)
    };
}

#[cfg(not(feature = "logging"))]
macro_rules! server_log_trace {
    ($($arg:tt)*) => {{
        let _ = core::format_args!($($arg)*);
    }};
}

// Make macros visible to child modules (register/).
pub(crate) use server_log_debug;
pub(crate) use server_log_trace;

// ---------------------------------------------------------------------------
// ServerServices struct
// ---------------------------------------------------------------------------

/// The Modbus server runtime.
///
/// Owns the transport and the application callback handler. Construct via
/// [`ServerServices::new`], call `connect()`, then drive `poll()` in a loop.
///
/// ## Generic Parameters
///
/// - `TRANSPORT`: implements [`Transport`].
/// - `APP`: implements [`ModbusAppHandler`].
/// - `QUEUE_DEPTH` *(default: `8`)*: maximum number of concurrently-buffered
///   requests **and** pending responses.  Increase on resource-rich targets;
///   decrease (or keep at `1`) on constrained embedded targets.
pub struct ServerServices<TRANSPORT, APP, const QUEUE_DEPTH: usize = 8> {
    /// The unit ID (TCP) or slave address (Serial) this server responds to.
    ///
    /// Frames addressed to any other unit are silently discarded without a response.
    /// Broadcast frames (address `0`) are silently discarded unless Serial
    /// broadcast writes are explicitly enabled in [`ResilienceConfig`].
    pub(super) slave_address: UnitIdOrSlaveAddr,
    pub(super) app: APP,
    /// Transport layer used for sending and receiving Modbus frames.
    pub(super) transport: TRANSPORT,
    /// Configuration for the Modbus server.
    pub(super) config: ModbusConfig,
    /// Internal buffer for partially-received frames.
    pub(super) rxed_frame: Vec<u8, MAX_ADU_FRAME_LEN>,
    /// Resilience configuration (timeouts, priority queue, retry policy).
    pub(super) resilience: ResilienceConfig,
    /// Priority-ordered queue for incoming requests.
    pub(super) request_queue: RequestQueue<QUEUE_DEPTH>,
    /// FIFO retry queue for responses that could not be sent immediately.
    pub(super) response_queue: ResponseQueue<QUEUE_DEPTH>,
    /// Number of responses dropped due to queue overflow.
    pub(super) dropped_response_count: u32,
    /// Number of requests rejected due to response queue back-pressure.
    pub(super) rejected_request_count: u32,
    /// Peak observed utilization of the response retry queue (as a count).
    pub(super) peak_response_queue_size: usize,
    /// Listen-only mode flag: when true, server silently discards all requests
    /// except FC08 (Diagnostics) with sub-function 0x0001 (Restart Communications).
    /// Set by FC08 sub-function 0x0004 (Force Listen Only Mode), cleared by 0x0001.
    pub(super) listen_only_mode: bool,
    /// FC0B communication event counter.
    #[cfg(feature = "diagnostics")]
    pub(super) comm_event_counter: u16,
    /// FC0C communication message count.
    #[cfg(feature = "diagnostics")]
    pub(super) comm_message_count: u16,
    /// Circular event log used by FC0C (max 64 events).
    #[cfg(feature = "diagnostics")]
    pub(super) comm_event_log: Deque<u8, 64>,
    /// Optional server statistics tracking (feature-gated).
    #[cfg(feature = "diagnostics-stats")]
    pub stats: crate::statistics::ServerStatistics,
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP> ServerServices<TRANSPORT, APP, 8>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Creates a new [`ServerServices`] with the default queue depth of `8`.
    ///
    /// Pass [`ResilienceConfig::default()`] to disable all resilience features.
    ///
    /// Call [`connect`](Self::connect) before polling.
    ///
    /// ## Custom queue depth
    ///
    /// To use a different queue depth, construct with an explicit type annotation
    /// and call [`ServerServices::with_queue_depth`]:
    ///
    /// ```ignore
    /// let server: ServerServices<_, _, 16> =
    ///     ServerServices::with_queue_depth(transport, app, config, addr, resilience);
    /// ```
    pub fn new(
        transport: TRANSPORT,
        app: APP,
        config: ModbusConfig,
        slave_address: UnitIdOrSlaveAddr,
        resilience: ResilienceConfig,
    ) -> Self {
        Self::with_queue_depth(transport, app, config, slave_address, resilience)
    }
}

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Creates a [`ServerServices`] with an explicitly-sized queue depth `QUEUE_DEPTH`.
    ///
    /// Prefer [`ServerServices::new`] for the default depth of `8`.  Use this
    /// constructor when you need a different depth:
    ///
    /// ```ignore
    /// let server: ServerServices<_, _, 16> =
    ///     ServerServices::with_queue_depth(transport, app, config, addr, resilience);
    /// ```
    pub fn with_queue_depth(
        transport: TRANSPORT,
        app: APP,
        config: ModbusConfig,
        slave_address: UnitIdOrSlaveAddr,
        resilience: ResilienceConfig,
    ) -> Self {
        Self {
            slave_address,
            app,
            transport,
            config,
            rxed_frame: Vec::new(),
            resilience,
            request_queue: RequestQueue::new(),
            response_queue: ResponseQueue::new(),
            dropped_response_count: 0,
            rejected_request_count: 0,
            peak_response_queue_size: 0,
            listen_only_mode: false,
            #[cfg(feature = "diagnostics")]
            comm_event_counter: 0,
            #[cfg(feature = "diagnostics")]
            comm_message_count: 0,
            #[cfg(feature = "diagnostics")]
            comm_event_log: Deque::new(),
            #[cfg(feature = "diagnostics-stats")]
            stats: crate::statistics::ServerStatistics::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Internal clock helper
    // -----------------------------------------------------------------------

    /// Returns the current time in milliseconds as provided by the configured
    /// [`ClockFn`](resilience::ClockFn), or `0` when no clock is available.
    #[inline]
    pub(super) fn now_ms(&self) -> u64 {
        match self.resilience.clock_fn {
            Some(f) => f(),
            None => 0,
        }
    }

    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Establishes the underlying transport connection.
    pub fn connect(&mut self) -> Result<(), MbusError>
    where
        TRANSPORT::Error: Into<MbusError>,
    {
        server_log_debug!("connecting transport");
        self.transport.connect(&self.config).map_err(|e| e.into())
    }

    /// Returns an immutable reference to the application callback handler.
    pub fn app(&self) -> &APP {
        &self.app
    }

    /// Returns whether the underlying transport currently considers itself connected.
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }

    /// Closes the underlying transport connection.
    pub fn disconnect(&mut self)
    where
        TRANSPORT::Error: Into<MbusError>,
    {
        self.rxed_frame = Vec::new();
        let _ = self.transport.disconnect();
    }

    /// Re-establishes the underlying transport connection using the existing configuration.
    pub fn reconnect(&mut self) -> Result<(), MbusError>
    where
        TRANSPORT::Error: Into<MbusError>,
    {
        self.rxed_frame = Vec::new();
        let _ = self.transport.disconnect();
        self.connect()
    }

    /// Returns the configured response timeout in milliseconds.
    ///
    /// Kept for parity with the client-side runtime and upcoming retry scheduling work.
    #[allow(dead_code)]
    fn response_timeout_ms(&self) -> u64 {
        match &self.config {
            ModbusConfig::Tcp(config) => config.response_timeout_ms as u64,
            ModbusConfig::Serial(config) => config.response_timeout_ms as u64,
        }
    }

    /// Returns the configured number of retries for outstanding requests.
    ///
    /// Kept for parity with the client-side runtime and upcoming retry scheduling work.
    #[allow(dead_code)]
    fn retry_attempts(&self) -> u8 {
        match &self.config {
            ModbusConfig::Tcp(config) => config.retry_attempts,
            ModbusConfig::Serial(config) => config.retry_attempts,
        }
    }

    /// Returns a shared reference to the resilience configuration.
    pub fn resilience(&self) -> &ResilienceConfig {
        &self.resilience
    }

    /// Returns `true` when the frame must be silently discarded due to address filtering.
    fn should_drop_for_address(&self, message: &ModbusMessage) -> bool {
        let wire_txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();
        let function_code = message.pdu.function_code();
        let wire_addr = unit_id_or_slave_addr.get();
        let own_addr = self.slave_address.get();

        if wire_addr == own_addr {
            return false;
        }

        if wire_addr == 0 {
            server_log_trace!(
                "ignoring broadcast frame: txn_id={}, fc=0x{:02X} (broadcast disabled or unsupported transport)",
                wire_txn_id,
                function_code as u8,
            );
        } else {
            server_log_trace!(
                "dropping misaddressed frame: txn_id={}, wire_addr={}, own_addr={}",
                wire_txn_id,
                wire_addr,
                own_addr,
            );
        }

        true
    }

    fn should_handle_broadcast_write(&self, message: &ModbusMessage) -> bool {
        if !self.resilience.enable_broadcast_writes
            || !message.unit_id_or_slave_addr().is_broadcast()
        {
            return false;
        }

        if !TRANSPORT::SUPPORTS_BROADCAST_WRITES {
            return false;
        }

        let serial_capable = TRANSPORT::TRANSPORT_TYPE.is_serial_type();

        if !serial_capable {
            return false;
        }

        self.is_supported_broadcast_write_function(message.pdu.function_code())
    }

    fn is_supported_broadcast_write_function(&self, function_code: FunctionCode) -> bool {
        match function_code {
            #[cfg(feature = "coils")]
            FunctionCode::WriteSingleCoil | FunctionCode::WriteMultipleCoils => true,
            #[cfg(feature = "holding-registers")]
            FunctionCode::WriteSingleRegister | FunctionCode::WriteMultipleRegisters => true,
            _ => false,
        }
    }

    fn dispatch_broadcast_write_no_response(&mut self, message: &ModbusMessage) {
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_no_response_count();

        server_log_trace!(
            "dispatching serial broadcast write with no response: txn_id={}, fc=0x{:02X}",
            message.transaction_id(),
            message.pdu.function_code() as u8,
        );

        match message.pdu.function_code() {
            #[cfg(feature = "coils")]
            FunctionCode::WriteSingleCoil => {
                self.handle_broadcast_write_single_coil_request(message)
            }
            #[cfg(feature = "holding-registers")]
            FunctionCode::WriteSingleRegister => {
                self.handle_broadcast_write_single_register_request(message)
            }
            #[cfg(feature = "coils")]
            FunctionCode::WriteMultipleCoils => {
                self.handle_broadcast_write_multiple_coils_request(message)
            }
            #[cfg(feature = "holding-registers")]
            FunctionCode::WriteMultipleRegisters => {
                self.handle_broadcast_write_multiple_registers_request(message)
            }
            _ => server_log_debug!(
                "unsupported broadcast write FC ignored: txn_id={}, fc=0x{:02X}",
                message.transaction_id(),
                message.pdu.function_code() as u8,
            ),
        }
    }

    /// Checks whether back-pressure should be applied to avoid response queue overflow.
    ///
    /// Returns `true` if the response queue utilization exceeds 80% and the configured
    /// overflow policy is `RejectRequest`.
    fn should_apply_back_pressure(&self) -> bool {
        if self.resilience.timeouts.overflow_policy != OverflowPolicy::RejectRequest {
            return false;
        }
        let capacity = QUEUE_DEPTH;
        let utilization = (self.response_queue.len() * 100) / capacity;
        utilization >= 80
    }

    /// Returns the number of requests currently waiting in the priority queue.
    pub fn pending_request_count(&self) -> usize {
        self.request_queue.len()
    }

    /// Returns the number of responses currently waiting in the retry queue.
    pub fn pending_response_count(&self) -> usize {
        self.response_queue.len()
    }

    /// Returns the number of responses that have been dropped due to queue overflow.
    pub fn dropped_response_count(&self) -> u32 {
        self.dropped_response_count
    }

    /// Returns the number of requests that have been rejected due to back-pressure.
    pub fn rejected_request_count(&self) -> u32 {
        self.rejected_request_count
    }

    /// Returns the peak utilization observed in the response retry queue.
    pub fn peak_response_queue_size(&self) -> usize {
        self.peak_response_queue_size
    }
}

// ---------------------------------------------------------------------------
// Exception response helper + send pipeline
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Attempts to send `frame` over the transport.
    ///
    /// On failure the frame is copied into the response retry queue (if space
    /// is available) so that [`poll`](Self::poll) can retry it on subsequent
    /// calls.  An optional `send_ms` threshold from [`ResilienceConfig`]
    /// produces a debug-level log if the send duration exceeds the limit.
    pub(super) fn try_send_or_queue(
        &mut self,
        frame: &[u8],
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) {
        let start = self.now_ms();
        match self.transport.send(frame) {
            Ok(_) => self.handle_send_success(txn_id, unit_id_or_slave_addr, start),
            Err(err) => self.handle_send_failure(frame, txn_id, unit_id_or_slave_addr, err),
        }
    }

    fn handle_send_success(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        start_ms: u64,
    ) {
        let _ = unit_id_or_slave_addr;
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_server_message_count();

        #[cfg(feature = "traffic")]
        self.app.on_tx_frame(txn_id, unit_id_or_slave_addr);

        let elapsed = self.now_ms().saturating_sub(start_ms);
        let threshold = self.resilience.timeouts.send_ms as u64;
        if threshold > 0 && elapsed > threshold {
            server_log_debug!(
                "txn_id={}: send took {}ms (threshold={}ms)",
                txn_id,
                elapsed,
                threshold
            );
        }
    }

    fn handle_send_failure(
        &mut self,
        frame: &[u8],
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        err: <TRANSPORT as Transport>::Error,
    ) {
        server_log_debug!(
            "txn_id={}: transport send failed ({:?}); queuing for retry",
            txn_id,
            err
        );

        #[cfg(feature = "traffic")]
        self.app
            .on_tx_error(txn_id, unit_id_or_slave_addr, MbusError::SendFailed);

        self.queue_response_for_retry(frame, txn_id, unit_id_or_slave_addr);
        self.update_peak_response_queue_size();
    }

    fn queue_response_for_retry(
        &mut self,
        frame: &[u8],
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) {
        let mut queued: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        if queued.extend_from_slice(frame).is_ok() {
            let queued_at = self.now_ms();
            if !self.response_queue.push_back(PendingResponse {
                frame: queued,
                txn_id,
                unit_id_or_slave_addr,
                retry_count: 0,
                queued_at_ms: queued_at,
            }) {
                server_log_debug!("txn_id={}: response queue full; dropping response", txn_id);
                self.dropped_response_count = self.dropped_response_count.saturating_add(1);
            }
        }
    }

    fn update_peak_response_queue_size(&mut self) {
        // Track peak queue utilization after queueing attempt
        let current_size = self.response_queue.len();
        if current_size > self.peak_response_queue_size {
            self.peak_response_queue_size = current_size;
        }
    }

    /// Builds and sends an exception ADU for a failed request.
    ///
    /// Exception code mapping is derived from the function code and the
    /// internal error category.
    pub(super) fn send_exception_response(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        error: MbusError,
    ) {
        #[cfg(feature = "traffic")]
        self.app.on_rx_error(txn_id, unit_id_or_slave_addr, error);

        let exception_code = function_code.exception_code_for_error(&error);

        self.app.on_exception(
            txn_id,
            unit_id_or_slave_addr,
            function_code,
            exception_code,
            error,
        );

        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_exception_error_count();

        let response = match exception::build_exception_adu(
            txn_id,
            unit_id_or_slave_addr,
            function_code,
            exception_code,
            TRANSPORT::TRANSPORT_TYPE,
        ) {
            Ok(adu) => adu,
            Err(err) => {
                server_log_debug!(
                    "FC{:02X}: failed to build exception ADU: {:?}",
                    function_code as u8,
                    err
                );
                return;
            }
        };

        server_log_trace!(
            "FC{:02X}: sending exception response with code 0x{:02X}",
            function_code as u8,
            exception_code as u8
        );
        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }
}

// ---------------------------------------------------------------------------
// Receive + dispatch pipeline
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    pub(super) fn dispatch_request(&mut self, message: &ModbusMessage) {
        let wire_txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();
        let function_code = message.pdu.function_code();

        if self.should_handle_broadcast_write(message) {
            self.dispatch_broadcast_write_no_response(message);
            return;
        }

        // -----------------------------------------------------------------------
        // Listen-only mode gate (Modbus spec behavior for FC08 sub-function 0x0004)
        //
        // When listen-only mode is active, the server must silently discard all
        // requests except FC08 (Diagnostics) with sub-function 0x0001 (Restart
        // Communications Option). No exception response is sent.
        // -----------------------------------------------------------------------
        if self.listen_only_mode && function_code != FunctionCode::Diagnostics {
            #[cfg(feature = "diagnostics-stats")]
            self.stats.increment_no_response_count();

            server_log_trace!(
                "listen-only mode active; silently discarding non-diagnostics request: txn_id={}, fc=0x{:02X}",
                wire_txn_id,
                function_code as u8,
            );
            return;
        }

        // -----------------------------------------------------------------------
        // Unit ID / slave address filtering (Modbus spec requirement)
        //
        // A Modbus server MUST only respond to frames addressed to its own unit ID.
        // All other unicast frames are silently discarded — sending an exception
        // response to a misaddressed frame is a protocol violation (another device
        // owns that address and the response would corrupt the bus).
        //
        // Broadcast (address 0):
        //   - Serial RTU/ASCII: when enabled, supported write function codes are
        //     dispatched without sending any response.
        //   - TCP: broadcast is rarely used in TCP Modbus and is discarded here.
        //
        // Note: TCP MBAP unit ID 0xFF is a legacy "not-used" marker that some TCP
        // stacks send. If your client uses 0xFF as a wildcard, configure the server
        // with slave_address = 0xFF.
        // -----------------------------------------------------------------------
        if self.should_drop_for_address(message) {
            return;
        }

        #[cfg(feature = "traffic")]
        self.app.on_rx_frame(wire_txn_id, unit_id_or_slave_addr);

        server_log_trace!(
            "dispatching response: txn_id={}, unit_id_or_slave_addr={}",
            wire_txn_id,
            unit_id_or_slave_addr.get(),
        );

        use FunctionCode::*;
        match function_code {
            #[cfg(feature = "coils")]
            ReadCoils => {
                self.handle_read_coils_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "discrete-inputs")]
            ReadDiscreteInputs => self.handle_read_discrete_inputs_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "holding-registers")]
            ReadHoldingRegisters => self.handle_read_holding_registers_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "input-registers")]
            ReadInputRegisters => self.handle_read_input_registers_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "coils")]
            WriteSingleCoil => {
                self.handle_write_single_coil_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "holding-registers")]
            WriteSingleRegister => self.handle_write_single_register_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "coils")]
            WriteMultipleCoils => self.handle_write_multiple_coils_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "holding-registers")]
            WriteMultipleRegisters => self.handle_write_multiple_registers_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "holding-registers")]
            MaskWriteRegister => {
                self.handle_mask_write_register_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "holding-registers")]
            ReadWriteMultipleRegisters => self.handle_read_write_multiple_registers_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "fifo")]
            ReadFifoQueue => {
                self.handle_read_fifo_queue_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "file-record")]
            ReadFileRecord => {
                self.handle_read_file_record_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "file-record")]
            WriteFileRecord => {
                self.handle_write_file_record_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "diagnostics")]
            ReadExceptionStatus => self.handle_read_exception_status_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "diagnostics")]
            Diagnostics => {
                self.handle_diagnostics_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "diagnostics")]
            GetCommEventCounter => self.handle_get_comm_event_counter_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            #[cfg(feature = "diagnostics")]
            GetCommEventLog => {
                self.handle_get_comm_event_log_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "diagnostics")]
            ReportServerId => {
                self.handle_report_server_id_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
            #[cfg(feature = "diagnostics")]
            EncapsulatedInterfaceTransport => self.handle_encapsulated_interface_transport_request(
                wire_txn_id,
                unit_id_or_slave_addr,
                message,
            ),
            _ => self.send_exception_response(
                wire_txn_id,
                unit_id_or_slave_addr,
                function_code,
                MbusError::InvalidFunctionCode,
            ),
        }
    }

    /// Main execution loop. Call this in a tight loop to receive and dispatch
    /// incoming Modbus requests.
    ///
    /// Each call to `poll` performs the following steps in order:
    ///
    /// 1. **Drain response queue** — retry any previously-failed sends.
    /// 2. **Receive** — read bytes from the transport into the internal buffer.
    /// 3. **Ingest frames** — parse complete frames; queue or dispatch depending
    ///    on [`ResilienceConfig::enable_priority_queue`].
    /// 4. **Expire stale queued requests** — discard requests that have exceeded
    ///    [`TimeoutConfig::request_deadline_ms`] (if configured).
    /// 5. **Dispatch queued requests** — process all buffered requests in
    ///    priority order (only reached when `enable_priority_queue` is `true`).
    pub fn poll(&mut self) {
        // Step 1 — retry queued responses from previous failed sends.
        self.drain_response_queue();

        // Step 2 — receive bytes from the transport.
        match self.transport.recv() {
            Ok(frame) => {
                self.append_to_rxed_frame(frame);
            }
            Err(err) => {
                self.handle_recv_error(err);
                return;
            }
        }

        // Step 3 — parse complete frames and either queue or dispatch them.
        self.process_rxed_frame();

        // Steps 4 & 5 — only relevant when the priority queue is active.
        if self.resilience.enable_priority_queue {
            self.expire_stale_queued_requests();

            // Dispatch all queued requests in priority order.
            while let Some(pending) = self.request_queue.pop_highest_priority() {
                self.dispatch_pending_request(pending);
            }
        }
    }

    /// Expires queued requests that have exceeded the configured request deadline.
    ///
    /// In strict mode, expired requests are converted to timeout exception
    /// responses. Otherwise, they are silently discarded.
    fn expire_stale_queued_requests(&mut self) {
        let deadline = self.resilience.timeouts.request_deadline_ms;
        if deadline == 0 {
            return;
        }

        let now = self.now_ms();
        if self.resilience.timeouts.strict_mode {
            let expired = self.request_queue.take_expired(now, deadline);
            if !expired.is_empty() {
                server_log_debug!("{} stale request(s) expired from queue", expired.len());
                for pending in expired {
                    self.handle_expired_request_strict(pending);
                }
            }
        } else {
            let expired = self.request_queue.expire_stale(now, deadline);
            if expired > 0 {
                server_log_debug!("{} stale request(s) expired from queue", expired);
            }
        }
    }

    /// Attempts to resend queued responses from previous poll cycles.
    ///
    /// Iterates at most `len` items (the queue length at call time) so that a
    /// persistent transport failure does not starve the receive path.
    ///
    /// When `TimeoutConfig::response_retry_interval_ms` is configured and a
    /// clock is available, each queued response is retried only after the
    /// configured interval has elapsed since its last enqueue/retry timestamp.
    fn drain_response_queue(&mut self) {
        let retry_interval_ms = self.resilience.timeouts.response_retry_interval_ms as u64;
        let has_clock = self.resilience.clock_fn.is_some();
        let pending_count = self.response_queue.len();
        for _ in 0..pending_count {
            let pending = match self.response_queue.pop_front() {
                Some(p) => p,
                None => break,
            };
            if self.should_drop_exhausted_response_retry(&pending) {
                continue;
            }

            if self.should_delay_response_retry(&pending, retry_interval_ms, has_clock) {
                let _ = self.response_queue.push_back(pending);
                // Preserve FIFO order: if the head is not due yet, later
                // items should wait as well.
                break;
            }

            if !self.try_send_queued_response(pending) {
                // Do not try more this poll; let the transport recover.
                break;
            }
        }
    }

    fn should_drop_exhausted_response_retry(&self, pending: &PendingResponse) -> bool {
        if pending.retry_count >= self.resilience.max_send_retries {
            server_log_debug!(
                "dropping queued response after {} retry attempt(s)",
                pending.retry_count
            );
            true
        } else {
            false
        }
    }

    fn should_delay_response_retry(
        &self,
        pending: &PendingResponse,
        retry_interval_ms: u64,
        has_clock: bool,
    ) -> bool {
        if retry_interval_ms == 0 || !has_clock {
            return false;
        }
        let elapsed = self.now_ms().saturating_sub(pending.queued_at_ms);
        elapsed < retry_interval_ms
    }

    fn try_send_queued_response(&mut self, pending: PendingResponse) -> bool {
        match self.transport.send(&pending.frame) {
            Ok(_) => {
                self.on_queued_response_retry_success(&pending);
                true
            }
            Err(err) => {
                self.on_queued_response_retry_failure(pending, err);
                false
            }
        }
    }

    fn on_queued_response_retry_success(&mut self, pending: &PendingResponse) {
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_server_message_count();

        #[cfg(feature = "traffic")]
        self.app
            .on_tx_frame(pending.txn_id, pending.unit_id_or_slave_addr);

        server_log_trace!(
            "queued response sent on retry attempt {}",
            pending.retry_count + 1
        );
    }

    fn on_queued_response_retry_failure(
        &mut self,
        mut pending: PendingResponse,
        err: <TRANSPORT as Transport>::Error,
    ) {
        #[cfg(feature = "traffic")]
        self.app.on_tx_error(
            pending.txn_id,
            pending.unit_id_or_slave_addr,
            MbusError::SendFailed,
        );

        server_log_debug!(
            "queued response retry {} failed: {:?}; requeueing",
            pending.retry_count + 1,
            err
        );
        pending.retry_count += 1;
        pending.queued_at_ms = self.now_ms();
        let _ = self.response_queue.push_back(pending);
    }

    /// Dispatches a request that was previously placed into the priority queue.
    ///
    /// Measures the dispatch duration and emits a debug log if it exceeds the
    /// configured [`TimeoutConfig::app_callback_ms`] threshold.
    fn dispatch_pending_request(&mut self, pending: PendingRequest) {
        let transport_type = TRANSPORT::TRANSPORT_TYPE;

        let expected_length =
            match derive_length_from_bytes(pending.frame.as_slice(), transport_type) {
                Some(len) => len,
                None => {
                    server_log_debug!("queued request: could not derive frame length; dropping");
                    return;
                }
            };

        let message =
            match common::decompile_adu_frame(&pending.frame[..expected_length], transport_type) {
                Ok(msg) => msg,
                Err(err) => {
                    server_log_debug!("queued request: decompile failed: {:?}; dropping", err);
                    return;
                }
            };

        let message = match self.reframe_message(message) {
            Some(m) => m,
            None => return,
        };

        let start = self.now_ms();
        self.dispatch_request(&message);
        let elapsed = self.now_ms().saturating_sub(start);
        let threshold = self.resilience.timeouts.app_callback_ms as u64;
        if threshold > 0 && elapsed > threshold {
            server_log_debug!(
                "app callback for queued request took {}ms (threshold={}ms)",
                elapsed,
                threshold
            );
        }
    }

    /// In strict timeout mode, sends an exception response for an expired
    /// queued request instead of silently dropping it.
    fn handle_expired_request_strict(&mut self, pending: PendingRequest) {
        let transport_type = TRANSPORT::TRANSPORT_TYPE;

        let expected_length =
            match derive_length_from_bytes(pending.frame.as_slice(), transport_type) {
                Some(len) => len,
                None => {
                    server_log_debug!("strict expiry: unable to derive frame length; dropping");
                    return;
                }
            };

        let message =
            match common::decompile_adu_frame(&pending.frame[..expected_length], transport_type) {
                Ok(msg) => msg,
                Err(err) => {
                    server_log_debug!(
                        "strict expiry: failed to decompile queued request: {:?}",
                        err
                    );
                    return;
                }
            };

        let message = match self.reframe_message(message) {
            Some(m) => m,
            None => return,
        };

        let txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();
        let function_code = message.pdu.function_code();
        self.send_exception_response(
            txn_id,
            unit_id_or_slave_addr,
            function_code,
            MbusError::Timeout,
        );
    }

    fn handle_recv_error(&mut self, err: <TRANSPORT as Transport>::Error) {
        let recv_error: MbusError = err.into();
        let is_connection_loss = matches!(
            recv_error,
            MbusError::ConnectionClosed
                | MbusError::ConnectionFailed
                | MbusError::ConnectionLost
                | MbusError::IoError
        ) || !self.transport.is_connected();

        if is_connection_loss {
            let _ = self.transport.disconnect();
            self.rxed_frame.clear();
        } else {
            server_log_trace!("non-fatal recv status during poll: {:?}", recv_error);
        }
    }

    fn process_rxed_frame(&mut self) {
        while !self.rxed_frame.is_empty() {
            match self.ingest_frame() {
                Ok(consumed) => {
                    self.drain_rxed_frame(consumed);
                }
                Err(MbusError::BufferTooSmall) => {
                    server_log_trace!(
                        "incomplete frame in rx buffer; waiting for more bytes (buffer_len={})",
                        self.rxed_frame.len()
                    );
                    break;
                }
                Err(err) => {
                    self.handle_parse_error(err);
                }
            }
        }
    }

    fn handle_parse_error(&mut self, err: MbusError) {
        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_comm_error_count();

        server_log_debug!(
            "frame parse/resync event: error={:?}, buffer_len={}; dropping 1 byte",
            err,
            self.rxed_frame.len()
        );
        let len = self.rxed_frame.len();
        if len > 1 {
            self.rxed_frame.copy_within(1.., 0);
            self.rxed_frame.truncate(len - 1);
        } else {
            self.rxed_frame.clear();
        }
    }

    fn drain_rxed_frame(&mut self, consumed: usize) {
        server_log_trace!(
            "ingested complete frame consuming {} bytes from rx buffer len {}",
            consumed,
            self.rxed_frame.len()
        );
        let len = self.rxed_frame.len();
        if consumed < len {
            self.rxed_frame.copy_within(consumed.., 0);
            self.rxed_frame.truncate(len - consumed);
        } else {
            self.rxed_frame.clear();
        }
    }

    fn append_to_rxed_frame(&mut self, frame: Vec<u8, MAX_ADU_FRAME_LEN>) {
        server_log_trace!("received {} transport bytes", frame.len());
        if self.rxed_frame.extend_from_slice(frame.as_slice()).is_err() {
            server_log_debug!(
                "received frame buffer overflow while appending {} bytes; clearing receive buffer",
                frame.len()
            );
            self.rxed_frame.clear();
        }
    }

    #[cfg(feature = "diagnostics")]
    fn record_comm_event(&mut self, event_code: u8) {
        self.comm_event_counter = self.comm_event_counter.saturating_add(1);
        self.comm_message_count = self.comm_message_count.saturating_add(1);

        if self.comm_event_log.push_back(event_code).is_err() {
            let _ = self.comm_event_log.pop_front();
            let _ = self.comm_event_log.push_back(event_code);
        }
    }

    fn ingest_frame(&mut self) -> Result<usize, MbusError> {
        let transport_type = TRANSPORT::TRANSPORT_TYPE;

        server_log_trace!(
            "attempting frame ingest: transport_type={:?}, buffer_len={}",
            transport_type,
            self.rxed_frame.len()
        );

        let expected_length =
            match derive_length_from_bytes(self.rxed_frame.as_slice(), transport_type) {
                Some(len) => len,
                None => return Err(MbusError::BufferTooSmall),
            };

        server_log_trace!("derived expected frame length={}", expected_length);

        if expected_length > MAX_ADU_FRAME_LEN {
            server_log_debug!(
                "derived frame length {} exceeds MAX_ADU_FRAME_LEN {}",
                expected_length,
                MAX_ADU_FRAME_LEN
            );
            return Err(MbusError::BasicParseError);
        }

        if self.rxed_frame.len() < expected_length {
            return Err(MbusError::BufferTooSmall);
        }

        let message = match self.parse_inbound_frame_body(expected_length) {
            Ok(msg) => msg,
            // For TCP transports with a fully-received frame, an unrecognised
            // function code should elicit an Illegal Function exception (per the
            // Modbus spec) rather than silently resyncing byte-by-byte.
            Err(MbusError::UnsupportedFunction(fc_byte)) => {
                use TransportType::*;
                if expected_length >= 8
                    && matches!(TRANSPORT::TRANSPORT_TYPE, StdTcp | CustomTcp)
                {
                    let txn_hi = self.rxed_frame[0];
                    let txn_lo = self.rxed_frame[1];
                    let unit_id = self.rxed_frame[6];
                    // MBAP(6) + PDU(2): txn(2) proto(2) len=3(2) unit(1) | exc_fc(1) exc_code(1)
                    let exc_frame: [u8; 9] = [
                        txn_hi, txn_lo, 0x00, 0x00, 0x00, 0x03,
                        unit_id,
                        fc_byte | 0x80,
                        0x01, // Illegal Function
                    ];
                    let _ = self.transport.send(&exc_frame);
                    return Ok(expected_length);
                }
                return Err(MbusError::UnsupportedFunction(fc_byte));
            }
            Err(err) => return Err(err),
        };

        use TransportType::*;
        let message = match TRANSPORT::TRANSPORT_TYPE {
            StdTcp | CustomTcp => {
                let mbap_header = match message.additional_address() {
                    AdditionalAddress::MbapHeader(header) => header,
                    _ => return Ok(expected_length),
                };
                let additional_addr = AdditionalAddress::MbapHeader(*mbap_header);
                ModbusMessage::new(additional_addr, message.pdu)
            }
            StdSerial(_) | CustomSerial(_) => {
                let slave_addr = match message.additional_address() {
                    AdditionalAddress::SlaveAddress(addr) => addr.address(),
                    _ => return Ok(expected_length),
                };
                let additional_address =
                    AdditionalAddress::SlaveAddress(SlaveAddress::new(slave_addr)?);
                ModbusMessage::new(additional_address, message.pdu)
            }
        };

        if self.should_handle_broadcast_write(&message) {
            self.dispatch_broadcast_write_no_response(&message);
            return Ok(expected_length);
        }

        if self.should_drop_for_address(&message) {
            return Ok(expected_length);
        }

        #[cfg(feature = "diagnostics")]
        self.record_comm_event(0x04);

        self.enqueue_or_dispatch_inbound(&message, expected_length);

        server_log_trace!("frame ingest complete for {} bytes", expected_length);
        Ok(expected_length)
    }

    /// Decompiles the raw ADU bytes and updates the diagnostics message counter.
    ///
    /// Returns the parsed [`ModbusMessage`] before transport-specific address
    /// normalisation is applied.  The caller is responsible for reframing the
    /// address once it has the concrete transport type in scope.
    fn parse_inbound_frame_body(
        &mut self,
        expected_length: usize,
    ) -> Result<ModbusMessage, MbusError> {
        let transport_type = TRANSPORT::TRANSPORT_TYPE;
        let message = match common::decompile_adu_frame(
            &self.rxed_frame[..expected_length],
            transport_type,
        ) {
            Ok(value) => value,
            Err(err) => {
                server_log_debug!(
                    "decompile_adu_frame failed for {} bytes: {:?}",
                    expected_length,
                    err
                );
                return Err(err);
            }
        };

        #[cfg(feature = "diagnostics-stats")]
        self.stats.increment_message_count();

        Ok(message)
    }

    /// Routes an already-validated inbound message to the priority queue or
    /// dispatches it immediately on the hot path.
    fn enqueue_or_dispatch_inbound(&mut self, message: &ModbusMessage, expected_length: usize) {
        if self.resilience.enable_priority_queue {
            self.try_enqueue_request(message, expected_length);
        } else {
            self.dispatch_immediately(message);
        }
    }

    /// Attempts to enqueue a validated inbound message into the priority queue.
    ///
    /// Applies back-pressure rejection when the queue is overloaded, and falls
    /// back to an immediate dispatch if the queue is full.  The
    /// `Vec<u8, MAX_ADU_FRAME_LEN>` raw-frame copy lives on this method's stack
    /// frame rather than on `ingest_frame`'s, reducing the peak stack depth of
    /// the receive path.
    fn try_enqueue_request(&mut self, message: &ModbusMessage, expected_length: usize) {
        if self.should_apply_back_pressure() {
            server_log_debug!(
                "txn_id={}: request rejected due to response queue back-pressure (utilization >= 80%)",
                message.transaction_id()
            );
            self.rejected_request_count = self.rejected_request_count.saturating_add(1);
            self.send_exception_response(
                message.transaction_id(),
                message.unit_id_or_slave_addr(),
                message.pdu.function_code(),
                MbusError::TooManyRequests,
            );
            return;
        }

        let priority = RequestPriority::from_function_code(message.pdu.function_code());
        let mut raw_frame: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = raw_frame.extend_from_slice(&self.rxed_frame[..expected_length]);
        let received_at = self.now_ms();
        if !self.request_queue.push(PendingRequest {
            frame: raw_frame,
            priority,
            received_at_ms: received_at,
        }) {
            server_log_debug!(
                "request queue full; dispatching immediately (priority={:?})",
                priority
            );
            let start = self.now_ms();
            self.dispatch_request(message);
            let elapsed = self.now_ms().saturating_sub(start);
            let threshold = self.resilience.timeouts.app_callback_ms as u64;
            if threshold > 0 && elapsed > threshold {
                server_log_debug!(
                    "app callback took {}ms (threshold={}ms) [queue-full fallback]",
                    elapsed,
                    threshold
                );
            }
        }
    }

    /// Hot-path dispatch: sends the response immediately and logs if the app
    /// callback exceeds the configured threshold.
    fn dispatch_immediately(&mut self, message: &ModbusMessage) {
        let start = self.now_ms();
        self.dispatch_request(message);
        let elapsed = self.now_ms().saturating_sub(start);
        let threshold = self.resilience.timeouts.app_callback_ms as u64;
        if threshold > 0 && elapsed > threshold {
            server_log_debug!(
                "app callback took {}ms (threshold={}ms)",
                elapsed,
                threshold
            );
        }
    }

    /// Re-frames a parsed `ModbusMessage` from raw bytes into the correct
    /// address variant for the active transport type.
    fn reframe_message(&self, message: ModbusMessage) -> Option<ModbusMessage> {
        use TransportType::*;
        match TRANSPORT::TRANSPORT_TYPE {
            StdTcp | CustomTcp => {
                let mbap_header = match message.additional_address() {
                    AdditionalAddress::MbapHeader(h) => h,
                    _ => return None,
                };
                Some(ModbusMessage::new(
                    AdditionalAddress::MbapHeader(*mbap_header),
                    message.pdu,
                ))
            }
            StdSerial(_) | CustomSerial(_) => {
                let addr = match message.additional_address() {
                    AdditionalAddress::SlaveAddress(s) => s.address(),
                    _ => return None,
                };
                let additional = AdditionalAddress::SlaveAddress(SlaveAddress::new(addr).ok()?);
                Some(ModbusMessage::new(additional, message.pdu))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HoldingRegisterMap;
    use mbus_macros::HoldingRegistersModel;

    #[derive(Debug, Default, HoldingRegistersModel)]
    #[reg(allow_gaps)]
    struct SparseHoldingRegisters {
        #[reg(addr = 0)]
        a: u16,
        #[reg(addr = 1000)]
        b: u16,
    }

    #[test]
    fn sparse_holding_registers_encode_single_word_at_low_address() {
        let mut regs = SparseHoldingRegisters::default();
        regs.set_a(0x1234);
        regs.set_b(0xABCD);

        let mut out = [0u8; 2];
        let written = regs.encode(0, 1, &mut out).expect("encode should succeed");

        assert_eq!(written, 2);
        assert_eq!(out, [0x12, 0x34]);
    }

    #[test]
    fn sparse_holding_registers_encode_single_word_at_high_address() {
        let mut regs = SparseHoldingRegisters::default();
        regs.set_a(0x1234);
        regs.set_b(0xABCD);

        let mut out = [0u8; 2];
        let written = regs
            .encode(1000, 1, &mut out)
            .expect("encode should succeed");

        assert_eq!(written, 2);
        assert_eq!(out, [0xAB, 0xCD]);
    }

    #[test]
    fn sparse_holding_registers_gap_request_returns_invalid_address() {
        let mut regs = SparseHoldingRegisters::default();
        regs.set_a(0x1234);
        regs.set_b(0xABCD);

        let mut out = [0u8; 4];
        let err = regs
            .encode(0, 2, &mut out)
            .expect_err("gap should fail with InvalidAddress");

        assert_eq!(err, MbusError::InvalidAddress);
    }
}
