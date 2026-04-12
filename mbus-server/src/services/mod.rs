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
pub mod exception;
pub mod framing;
#[cfg(any(feature = "holding-registers", feature = "input-registers"))]
pub mod register;

use crate::app::ModbusAppHandler;
use heapless::Vec;
use mbus_core::{
    data_unit::common::{
        self, MAX_ADU_FRAME_LEN, ModbusMessage, SlaveAddress, derive_length_from_bytes,
    },
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{ModbusConfig, Transport, UnitIdOrSlaveAddr},
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
pub struct ServerServices<TRANSPORT, APP> {
    /// The unit ID (TCP) or slave address (Serial) this server responds to.
    ///
    /// Frames addressed to any other unit are silently discarded without a response.
    /// Broadcast frames (address `0`) are also silently discarded; full broadcast
    /// write forwarding for Serial transports is not yet implemented.
    pub(super) slave_address: UnitIdOrSlaveAddr,
    pub(super) app: APP,
    /// Transport layer used for sending and receiving Modbus frames.
    pub(super) transport: TRANSPORT,
    /// Configuration for the Modbus server.
    pub(super) config: ModbusConfig,
    /// Internal buffer for partially-received frames.
    pub(super) rxed_frame: Vec<u8, MAX_ADU_FRAME_LEN>,
}

// ---------------------------------------------------------------------------
// Lifecycle
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP> ServerServices<TRANSPORT, APP>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Creates a new [`ServerServices`] with the provided transport, application
    /// handler, configuration, and slave address.
    ///
    /// Call [`connect`](Self::connect) before polling.
    pub fn new(
        transport: TRANSPORT,
        app: APP,
        config: ModbusConfig,
        slave_address: UnitIdOrSlaveAddr,
    ) -> Self {
        Self {
            slave_address,
            app,
            transport,
            config,
            rxed_frame: Vec::new(),
        }
    }

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
}

// ---------------------------------------------------------------------------
// Receive pipeline
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Exception response helper
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP> ServerServices<TRANSPORT, APP>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
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
        let exception_code = function_code.exception_code_for_error(&error);

        let response = match exception::build_exception_adu(
            txn_id,
            unit_id_or_slave_addr,
            function_code,
            exception_code,
            self.transport.transport_type(),
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

        if let Err(err) = self.transport.send(&response) {
            server_log_debug!(
                "FC{:02X}: failed to send exception response: {:?}",
                function_code as u8,
                err
            );
        } else {
            server_log_trace!(
                "FC{:02X}: sent exception response with code 0x{:02X}",
                function_code as u8,
                exception_code as u8
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Receive pipeline
// ---------------------------------------------------------------------------

impl<TRANSPORT, APP> ServerServices<TRANSPORT, APP>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    pub(super) fn dispatch_request(&mut self, message: &ModbusMessage) {
        let wire_txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();
        let function_code = message.pdu.function_code();

        // -----------------------------------------------------------------------
        // Unit ID / slave address filtering (Modbus spec requirement)
        //
        // A Modbus server MUST only respond to frames addressed to its own unit ID.
        // All other unicast frames are silently discarded — sending an exception
        // response to a misaddressed frame is a protocol violation (another device
        // owns that address and the response would corrupt the bus).
        //
        // Broadcast (address 0):
        //   - Serial RTU/ASCII: write function codes should be processed without
        //     sending a response. Full broadcast write forwarding is not yet
        //     implemented; broadcast frames are currently discarded so that a
        //     partial implementation never sends accidental responses.
        //   - TCP: broadcast is rarely used in TCP Modbus and is discarded here.
        //
        // Note: TCP MBAP unit ID 0xFF is a legacy "not-used" marker that some TCP
        // stacks send. If your client uses 0xFF as a wildcard, configure the server
        // with slave_address = 0xFF.
        // -----------------------------------------------------------------------
        let wire_addr = unit_id_or_slave_addr.get();
        let own_addr = self.slave_address.get();
        if wire_addr != own_addr {
            if wire_addr == 0 {
                server_log_trace!(
                    "ignoring broadcast frame: txn_id={}, fc=0x{:02X} (broadcast write forwarding not yet implemented)",
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
            return;
        }

        #[cfg(feature = "traffic")]
        self.app.on_rx_frame(wire_txn_id, unit_id_or_slave_addr);

        server_log_trace!(
            "dispatching response: txn_id={}, unit_id_or_slave_addr={}",
            wire_txn_id,
            unit_id_or_slave_addr.get(),
        );

        use mbus_core::function_codes::public::FunctionCode::*;
        match function_code {
            #[cfg(feature = "coils")]
            ReadCoils => {
                self.handle_read_coils_request(wire_txn_id, unit_id_or_slave_addr, message)
            }
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
            // MaskWriteRegister => ,
            // ReadWriteMultipleRegisters => ,
            // ReadDiscreteInputs => ,
            // ReadFifoQueue => ,
            // ReadFileRecord => ,
            // WriteFileRecord => ,
            // ReadExceptionStatus => ,
            // Diagnostics => ,
            // GetCommEventCounter => ,
            // GetCommEventLog => ,
            // ReportServerId => ,
            // EncapsulatedInterfaceTransport => ,
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
    pub fn poll(&mut self) {
        match self.transport.recv() {
            Ok(frame) => {
                self.append_to_rxed_frame(frame);
                self.process_rxed_frame();
            }
            Err(err) => {
                self.handle_recv_error(err);
            }
        }
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

    fn ingest_frame(&mut self) -> Result<usize, MbusError> {
        let frame = self.rxed_frame.as_slice();
        let transport_type = self.transport.transport_type();

        server_log_trace!(
            "attempting frame ingest: transport_type={:?}, buffer_len={}",
            transport_type,
            frame.len()
        );

        let expected_length = match derive_length_from_bytes(frame, transport_type) {
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

        let message = match common::decompile_adu_frame(&frame[..expected_length], transport_type) {
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

        use mbus_core::data_unit::common::AdditionalAddress;
        use mbus_core::transport::TransportType::*;
        let message = match self.transport.transport_type() {
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

        self.dispatch_request(&message);
        server_log_trace!("frame dispatch complete for {} bytes", expected_length);

        Ok(expected_length)
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
