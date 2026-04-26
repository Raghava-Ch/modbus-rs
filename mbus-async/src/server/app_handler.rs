//! [`AsyncAppHandler`] trait, [`ModbusRequest`] and [`ModbusResponse`] types.
//!
//! These are the central types for the async server API. User applications either:
//! - Implement [`AsyncAppHandler`] manually (Level 2 — full control), or
//! - Let the `#[async_modbus_app]` macro generate the impl (Level 1 — zero boilerplate).

use heapless::Vec;
use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN, Pdu, compile_adu_frame};
use mbus_core::errors::{ExceptionCode, MbusError};
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::EncapsulatedInterfaceType;
use mbus_core::function_codes::public::FunctionCode;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::{FileRecordReadSubRequest, MAX_SUB_REQUESTS_PER_PDU};
use mbus_core::transport::{checksum, SerialMode, TransportType, UnitIdOrSlaveAddr};
use std::future::Future;

/// Direction of a Modbus traffic event — mirrors [`mbus_server::TrafficDirection`] for
/// async server users who do not depend on `mbus-server` directly.
///
/// Exported alongside [`AsyncTrafficNotifier`] for convenience.
#[cfg(feature = "traffic")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AsyncTrafficDirection {
    /// Outgoing response ADU sent by the server.
    Tx,
    /// Incoming request ADU received by the server.
    Rx,
}

/// Error type for async server operations.
#[derive(Debug)]
pub enum AsyncServerError {
    /// A transport-level I/O error.
    Transport(MbusError),
    /// The remote client closed the connection.
    ConnectionClosed,
    /// A received frame could not be parsed as a valid Modbus ADU.
    FramingError(MbusError),
    /// The server failed to bind to the requested address (e.g. port already in use).
    BindFailed(std::io::Error),
}

impl core::fmt::Display for AsyncServerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AsyncServerError::Transport(e) => write!(f, "transport error: {e}"),
            AsyncServerError::ConnectionClosed => write!(f, "connection closed"),
            AsyncServerError::FramingError(e) => write!(f, "framing error: {e}"),
            AsyncServerError::BindFailed(e) => write!(f, "bind failed: {e}"),
        }
    }
}

impl std::error::Error for AsyncServerError {}

impl From<MbusError> for AsyncServerError {
    fn from(e: MbusError) -> Self {
        match e {
            MbusError::ConnectionClosed => AsyncServerError::ConnectionClosed,
            other => AsyncServerError::Transport(other),
        }
    }
}

// ── Auxiliary types ──────────────────────────────────────────────────────────

/// An owned write sub-request parsed from an FC15 (Write File Record) PDU.
///
/// Unlike [`mbus_core::models::file_record::FileRecordWriteSubRequest`], this type
/// owns the record data bytes so it can be held in a [`ModbusRequest`] variant.
#[cfg(feature = "file-record")]
#[derive(Debug, Clone)]
pub struct AsyncFileRecordWriteSubRequest {
    /// File number to write (0x0001–0xFFFF).
    pub file_number: u16,
    /// Starting record number within the file.
    pub record_number: u16,
    /// Number of 16-bit register words to write.
    pub record_length: u16,
    /// Raw big-endian record bytes (`record_length * 2` bytes).
    pub record_data: Vec<u8, MAX_ADU_FRAME_LEN>,
}

// ── ModbusRequest ────────────────────────────────────────────────────────────

/// A parsed, typed Modbus request received from a client.
///
/// Produced by the session loop inside [`AsyncServerSession::run`](super::session::AsyncServerSession::run).
/// Feature flags mirror `mbus-server` exactly: each variant is only present when the
/// corresponding feature is enabled.
// Variants use heapless fixed-size buffers (stack-allocated) and are consumed
// once per request; boxing the large variant would change the public API.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
#[non_exhaustive]
pub enum ModbusRequest {
    /// FC01 — Read Coils.
    #[cfg(feature = "coils")]
    ReadCoils {
        /// Transaction ID (TCP) or 0 (serial).
        txn_id: u16,
        /// Addressed unit / slave.
        unit: UnitIdOrSlaveAddr,
        /// Starting coil address.
        address: u16,
        /// Number of coils to read.
        count: u16,
    },
    /// FC05 — Write Single Coil.
    #[cfg(feature = "coils")]
    WriteSingleCoil {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Coil address.
        address: u16,
        /// Value to write.
        value: bool,
    },
    /// FC0F — Write Multiple Coils.
    #[cfg(feature = "coils")]
    WriteMultipleCoils {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of coils.
        count: u16,
        /// Packed coil bytes (LSB of first byte = coil at `address`).
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// FC02 — Read Discrete Inputs.
    #[cfg(feature = "discrete-inputs")]
    ReadDiscreteInputs {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of inputs.
        count: u16,
    },
    /// FC03 — Read Holding Registers.
    #[cfg(feature = "registers")]
    ReadHoldingRegisters {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Register count.
        count: u16,
    },
    /// FC06 — Write Single Register.
    #[cfg(feature = "registers")]
    WriteSingleRegister {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Register address.
        address: u16,
        /// Value to write.
        value: u16,
    },
    /// FC10 — Write Multiple Registers.
    #[cfg(feature = "registers")]
    WriteMultipleRegisters {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Register count.
        count: u16,
        /// Register data bytes (big-endian, 2 bytes per register).
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// FC04 — Read Input Registers.
    #[cfg(feature = "registers")]
    ReadInputRegisters {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Register count.
        count: u16,
    },
    /// FC16 — Mask Write Register.
    #[cfg(feature = "registers")]
    MaskWriteRegister {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Register address.
        address: u16,
        /// AND mask applied to the current register value.
        and_mask: u16,
        /// OR mask applied after the AND mask.
        or_mask: u16,
    },
    /// FC17 — Read/Write Multiple Registers.
    #[cfg(feature = "registers")]
    ReadWriteMultipleRegisters {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Starting address of the read window.
        read_address: u16,
        /// Number of registers to read (1–125).
        read_count: u16,
        /// Starting address of the write window.
        write_address: u16,
        /// Number of registers to write (1–121).
        write_count: u16,
        /// Write register data bytes (big-endian, 2 bytes per register).
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// FC07 — Read Exception Status (serial-line only per Modbus spec).
    #[cfg(feature = "diagnostics")]
    ReadExceptionStatus {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
    },
    /// FC08 — Diagnostics.
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Raw sub-function code (use `DiagnosticSubFunction::try_from` to convert).
        sub_function: u16,
        /// Data word accompanying the sub-function.
        data: u16,
    },
    /// FC0B — Get Comm Event Counter (serial-line only per Modbus spec).
    #[cfg(feature = "diagnostics")]
    GetCommEventCounter {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
    },
    /// FC0C — Get Comm Event Log (serial-line only per Modbus spec).
    #[cfg(feature = "diagnostics")]
    GetCommEventLog {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
    },
    /// FC11 — Report Server ID (serial-line only per Modbus spec).
    #[cfg(feature = "diagnostics")]
    ReportServerId {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
    },
    /// FC2B — Encapsulated Interface Transport.
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Raw MEI type byte (e.g. `0x0E` = Read Device Identification).
        mei_type: u8,
        /// Payload bytes following the MEI type field.
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// FC18 — Read FIFO Queue.
    #[cfg(feature = "fifo")]
    ReadFifoQueue {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// FIFO pointer address.
        pointer_address: u16,
    },
    /// FC14 — Read File Record.
    #[cfg(feature = "file-record")]
    ReadFileRecord {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Parsed read sub-requests (one per file/record range requested).
        sub_requests: Vec<FileRecordReadSubRequest, MAX_SUB_REQUESTS_PER_PDU>,
    },
    /// FC15 — Write File Record.
    #[cfg(feature = "file-record")]
    WriteFileRecord {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// Parsed write sub-requests with owned record data.
        sub_requests: Vec<AsyncFileRecordWriteSubRequest, MAX_SUB_REQUESTS_PER_PDU>,
        /// Raw PDU data bytes — pass to [`ModbusResponse::echo_write_file_record`] to build the success response.
        raw_pdu_data: Vec<u8, MAX_PDU_DATA_LEN>,
    },
    /// Any function code not covered by the active feature flags, or unknown.
    Unknown {
        /// Transaction ID.
        txn_id: u16,
        /// Addressed unit.
        unit: UnitIdOrSlaveAddr,
        /// The raw function code byte.
        function_code: u8,
    },
}

impl ModbusRequest {
    /// The transaction ID carried by this request.
    pub fn txn_id(&self) -> u16 {
        match self {
            #[cfg(feature = "coils")]
            ModbusRequest::ReadCoils { txn_id, .. } => *txn_id,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteSingleCoil { txn_id, .. } => *txn_id,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteMultipleCoils { txn_id, .. } => *txn_id,
            #[cfg(feature = "discrete-inputs")]
            ModbusRequest::ReadDiscreteInputs { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadHoldingRegisters { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteSingleRegister { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteMultipleRegisters { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadInputRegisters { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::MaskWriteRegister { txn_id, .. } => *txn_id,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadWriteMultipleRegisters { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReadExceptionStatus { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::Diagnostics { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventCounter { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventLog { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReportServerId { txn_id, .. } => *txn_id,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::EncapsulatedInterfaceTransport { txn_id, .. } => *txn_id,
            #[cfg(feature = "fifo")]
            ModbusRequest::ReadFifoQueue { txn_id, .. } => *txn_id,
            #[cfg(feature = "file-record")]
            ModbusRequest::ReadFileRecord { txn_id, .. } => *txn_id,
            #[cfg(feature = "file-record")]
            ModbusRequest::WriteFileRecord { txn_id, .. } => *txn_id,
            ModbusRequest::Unknown { txn_id, .. } => *txn_id,
        }
    }

    /// The addressed unit / slave for this request.
    pub fn unit(&self) -> UnitIdOrSlaveAddr {
        match self {
            #[cfg(feature = "coils")]
            ModbusRequest::ReadCoils { unit, .. } => *unit,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteSingleCoil { unit, .. } => *unit,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteMultipleCoils { unit, .. } => *unit,
            #[cfg(feature = "discrete-inputs")]
            ModbusRequest::ReadDiscreteInputs { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadHoldingRegisters { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteSingleRegister { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteMultipleRegisters { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadInputRegisters { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::MaskWriteRegister { unit, .. } => *unit,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadWriteMultipleRegisters { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReadExceptionStatus { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::Diagnostics { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventCounter { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventLog { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReportServerId { unit, .. } => *unit,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::EncapsulatedInterfaceTransport { unit, .. } => *unit,
            #[cfg(feature = "fifo")]
            ModbusRequest::ReadFifoQueue { unit, .. } => *unit,
            #[cfg(feature = "file-record")]
            ModbusRequest::ReadFileRecord { unit, .. } => *unit,
            #[cfg(feature = "file-record")]
            ModbusRequest::WriteFileRecord { unit, .. } => *unit,
            ModbusRequest::Unknown { unit, .. } => *unit,
        }
    }

    /// The function code byte for this request (useful for logging and diagnostics).
    pub fn function_code_byte(&self) -> u8 {
        match self {
            #[cfg(feature = "coils")]
            ModbusRequest::ReadCoils { .. } => FunctionCode::ReadCoils as u8,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteSingleCoil { .. } => FunctionCode::WriteSingleCoil as u8,
            #[cfg(feature = "coils")]
            ModbusRequest::WriteMultipleCoils { .. } => FunctionCode::WriteMultipleCoils as u8,
            #[cfg(feature = "discrete-inputs")]
            ModbusRequest::ReadDiscreteInputs { .. } => FunctionCode::ReadDiscreteInputs as u8,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadHoldingRegisters { .. } => FunctionCode::ReadHoldingRegisters as u8,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteSingleRegister { .. } => FunctionCode::WriteSingleRegister as u8,
            #[cfg(feature = "registers")]
            ModbusRequest::WriteMultipleRegisters { .. } => {
                FunctionCode::WriteMultipleRegisters as u8
            }
            #[cfg(feature = "registers")]
            ModbusRequest::ReadInputRegisters { .. } => FunctionCode::ReadInputRegisters as u8,
            #[cfg(feature = "registers")]
            ModbusRequest::MaskWriteRegister { .. } => FunctionCode::MaskWriteRegister as u8,
            #[cfg(feature = "registers")]
            ModbusRequest::ReadWriteMultipleRegisters { .. } => {
                FunctionCode::ReadWriteMultipleRegisters as u8
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReadExceptionStatus { .. } => FunctionCode::ReadExceptionStatus as u8,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::Diagnostics { .. } => FunctionCode::Diagnostics as u8,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventCounter { .. } => FunctionCode::GetCommEventCounter as u8,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventLog { .. } => FunctionCode::GetCommEventLog as u8,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReportServerId { .. } => FunctionCode::ReportServerId as u8,
            #[cfg(feature = "diagnostics")]
            ModbusRequest::EncapsulatedInterfaceTransport { .. } => {
                FunctionCode::EncapsulatedInterfaceTransport as u8
            }
            #[cfg(feature = "fifo")]
            ModbusRequest::ReadFifoQueue { .. } => FunctionCode::ReadFifoQueue as u8,
            #[cfg(feature = "file-record")]
            ModbusRequest::ReadFileRecord { .. } => FunctionCode::ReadFileRecord as u8,
            #[cfg(feature = "file-record")]
            ModbusRequest::WriteFileRecord { .. } => FunctionCode::WriteFileRecord as u8,
            ModbusRequest::Unknown { function_code, .. } => *function_code,
        }
    }
}

// ── ModbusResponse ───────────────────────────────────────────────────────────

/// A Modbus response to be sent back to the client.
///
/// Produced by user application logic and consumed by
/// [`AsyncServerSession::respond()`](super::session::AsyncServerSession::respond).
#[derive(Debug)]
pub enum ModbusResponse {
    /// A byte-count-prefixed payload (FC01, FC02, FC03, FC04).
    ByteCountPayload {
        /// The function code for this response.
        fc: FunctionCode,
        /// The response bytes (packed bits or big-endian register words).
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// Echo of a single coil write (FC05).
    EchoCoil {
        /// Coil address.
        address: u16,
        /// Coil value (0xFF00 = ON, 0x0000 = OFF per Modbus spec).
        raw_value: u16,
    },
    /// Echo of a single register write (FC06).
    EchoRegister {
        /// Register address.
        address: u16,
        /// Register value.
        value: u16,
    },
    /// Echo of a mask-write register (FC16): address, AND mask, OR mask.
    EchoMaskWrite {
        /// Register address.
        address: u16,
        /// AND mask.
        and_mask: u16,
        /// OR mask.
        or_mask: u16,
    },
    /// Echo of a multi-write (FC0F, FC10).
    EchoMultiWrite {
        /// The function code (FC0F or FC10).
        fc: FunctionCode,
        /// Starting address.
        address: u16,
        /// Quantity written.
        count: u16,
    },
    /// A single raw byte response (FC07 Read Exception Status).
    #[cfg(feature = "diagnostics")]
    SingleByte {
        /// Function code.
        fc: FunctionCode,
        /// The single response byte.
        value: u8,
    },
    /// Echo of a Diagnostics request (FC08): sub-function + result data word.
    #[cfg(feature = "diagnostics")]
    DiagnosticsEcho {
        /// Sub-function code (echoed from request).
        sub_function: u16,
        /// Result data word.
        result: u16,
    },
    /// Two-u16 response (FC0B Get Comm Event Counter).
    #[cfg(feature = "diagnostics")]
    TwoU16 {
        /// Function code.
        fc: FunctionCode,
        /// First u16 value.
        first: u16,
        /// Second u16 value.
        second: u16,
    },
    /// FIFO queue response (FC18): app payload `[fifo_count_hi, fifo_count_lo, value0..]`.
    #[cfg(feature = "fifo")]
    FifoData {
        /// App payload bytes: `[fifo_count(2 BE), values...]`.
        data: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// Read Device Identification response (FC2B / MEI 0x0E).
    #[cfg(feature = "diagnostics")]
    ReadDeviceId {
        /// Read Device ID code (0x01–0x04).
        read_device_id_code: u8,
        /// Conformity level byte.
        conformity_level: u8,
        /// Whether more objects follow in a subsequent request.
        more_follows: bool,
        /// Object ID to start with in next request (0 if no more follows).
        next_object_id: u8,
        /// Concatenated object triples: `[id(1), len(1), value(N)...]`.
        objects: Vec<u8, MAX_ADU_FRAME_LEN>,
    },
    /// Echo of a Write File Record request (FC15): raw PDU data bytes.
    #[cfg(feature = "file-record")]
    FileRecordWriteEcho {
        /// PDU data bytes from the original write request.
        pdu_data: Vec<u8, MAX_PDU_DATA_LEN>,
    },
    /// A Modbus exception response.
    Exception {
        /// The original request function code.
        request_fc: FunctionCode,
        /// The exception code to send.
        code: ExceptionCode,
    },
    /// A Modbus exception response for an unrecognised function code byte.
    ///
    /// Use this when the request arrived as [`ModbusRequest::Unknown`] and you
    /// need to reply with the correct exception FC byte (`fc_byte | 0x80`).
    /// Unlike [`Exception`], this variant accepts any raw `u8` function-code
    /// byte and does not require a [`FunctionCode`] enum value.
    ExceptionRaw {
        /// The raw function-code byte from the unknown request.
        fc_byte: u8,
        /// The exception code to send.
        code: ExceptionCode,
    },
    /// Suppress — send no response (e.g. for broadcast writes on serial).
    NoResponse,
}

impl ModbusResponse {
    // ── convenience constructors ─────────────────────────────────────────────

    /// Build a read-register response (FC03 / FC04) from a `u16` slice.
    pub fn registers(fc: FunctionCode, values: &[u16]) -> Self {
        let mut data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        for v in values {
            let _ = data.extend_from_slice(&v.to_be_bytes());
        }
        ModbusResponse::ByteCountPayload { fc, data }
    }

    /// Build a read-coil / read-discrete-input response (FC01 / FC02) from packed bytes.
    pub fn packed_bits(fc: FunctionCode, bytes: &[u8]) -> Self {
        let mut data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = data.extend_from_slice(bytes);
        ModbusResponse::ByteCountPayload { fc, data }
    }

    /// Echo a single-coil write (FC05).
    pub fn echo_coil(address: u16, on: bool) -> Self {
        ModbusResponse::EchoCoil {
            address,
            raw_value: if on { 0xFF00 } else { 0x0000 },
        }
    }

    /// Echo a single-register write (FC06).
    pub fn echo_register(address: u16, value: u16) -> Self {
        ModbusResponse::EchoRegister { address, value }
    }

    /// Echo a mask-write register (FC16).
    pub fn echo_mask_write(address: u16, and_mask: u16, or_mask: u16) -> Self {
        ModbusResponse::EchoMaskWrite {
            address,
            and_mask,
            or_mask,
        }
    }

    /// Echo a multi-write (FC0F / FC10).
    pub fn echo_multi_write(fc: FunctionCode, address: u16, count: u16) -> Self {
        ModbusResponse::EchoMultiWrite { fc, address, count }
    }

    /// Return a Modbus exception.
    pub fn exception(request_fc: FunctionCode, code: ExceptionCode) -> Self {
        ModbusResponse::Exception { request_fc, code }
    }

    /// Return a Modbus exception for an unrecognised raw function-code byte.
    ///
    /// This is the correct way to respond to [`ModbusRequest::Unknown`]
    /// requests: the exception response uses `fc_byte | 0x80` as the
    /// response function-code byte, matching the Modbus spec for any
    /// function code — including vendor-specific ones not present in the
    /// [`FunctionCode`] enum.
    pub fn exception_raw(fc_byte: u8, code: ExceptionCode) -> Self {
        ModbusResponse::ExceptionRaw { fc_byte, code }
    }

    /// Return `InvalidFunctionCode` exception for the given FC.
    pub fn invalid_function(request_fc: FunctionCode) -> Self {
        Self::exception(request_fc, ExceptionCode::IllegalFunction)
    }

    /// Single-byte response for FC07 (Read Exception Status).
    #[cfg(feature = "diagnostics")]
    pub fn read_exception_status(status: u8) -> Self {
        ModbusResponse::SingleByte {
            fc: FunctionCode::ReadExceptionStatus,
            value: status,
        }
    }

    /// Diagnostics echo for FC08: echo sub-function + result.
    #[cfg(feature = "diagnostics")]
    pub fn diagnostics_echo(sub_function: u16, result: u16) -> Self {
        ModbusResponse::DiagnosticsEcho {
            sub_function,
            result,
        }
    }

    /// Two-u16 response for FC0B (Get Comm Event Counter).
    #[cfg(feature = "diagnostics")]
    pub fn comm_event_counter(status_word: u16, event_count: u16) -> Self {
        ModbusResponse::TwoU16 {
            fc: FunctionCode::GetCommEventCounter,
            first: status_word,
            second: event_count,
        }
    }

    /// Byte-count-prefixed payload for FC0C (Get Comm Event Log).
    ///
    /// `payload` = `[status_hi, status_lo, event_count_hi, event_count_lo,
    ///               msg_count_hi, msg_count_lo, events...]`
    #[cfg(feature = "diagnostics")]
    pub fn comm_event_log(payload: &[u8]) -> Self {
        let mut data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = data.extend_from_slice(payload);
        ModbusResponse::ByteCountPayload {
            fc: FunctionCode::GetCommEventLog,
            data,
        }
    }

    /// Byte-count-prefixed payload for FC11 (Report Server ID).
    ///
    /// `payload` = `[server_id_bytes..., run_indicator_status]`
    #[cfg(feature = "diagnostics")]
    pub fn report_server_id(payload: &[u8]) -> Self {
        let mut data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = data.extend_from_slice(payload);
        ModbusResponse::ByteCountPayload {
            fc: FunctionCode::ReportServerId,
            data,
        }
    }

    /// Read Device Identification response (FC2B / MEI 0x0E).
    ///
    /// `objects` is a sequence of `[id(1), len(1), value(len)...]` triples.
    #[cfg(feature = "diagnostics")]
    pub fn read_device_id(
        read_device_id_code: u8,
        conformity_level: u8,
        more_follows: bool,
        next_object_id: u8,
        objects: &[u8],
    ) -> Self {
        let mut objects_buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = objects_buf.extend_from_slice(objects);
        ModbusResponse::ReadDeviceId {
            read_device_id_code,
            conformity_level,
            more_follows,
            next_object_id,
            objects: objects_buf,
        }
    }

    /// FIFO queue response (FC18).
    ///
    /// `data` is the app payload: `[fifo_count_hi, fifo_count_lo, value0_hi, value0_lo, ...]`.
    #[cfg(feature = "fifo")]
    pub fn fifo_response(data: &[u8]) -> Self {
        let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = buf.extend_from_slice(data);
        ModbusResponse::FifoData { data: buf }
    }

    /// Byte-count-prefixed response for FC14 (Read File Record).
    ///
    /// `payload` = concatenated sub-response blocks: `[sub_len(1), ref_type=0x06(1), data...]...`
    #[cfg(feature = "file-record")]
    pub fn read_file_record_response(payload: &[u8]) -> Self {
        let mut data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let _ = data.extend_from_slice(payload);
        ModbusResponse::ByteCountPayload {
            fc: FunctionCode::ReadFileRecord,
            data,
        }
    }

    /// Echo response for FC15 (Write File Record): pass the `raw_pdu_data` from the request.
    #[cfg(feature = "file-record")]
    pub fn echo_write_file_record(pdu_data: Vec<u8, MAX_PDU_DATA_LEN>) -> Self {
        ModbusResponse::FileRecordWriteEcho { pdu_data }
    }

    // ── internal: encode to ADU bytes ────────────────────────────────────────

    /// Encode into a complete ADU frame ready for transmission.
    pub(crate) fn encode(
        self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        match self {
            ModbusResponse::ByteCountPayload { fc, data } => {
                encode_byte_count_payload(fc, &data, txn_id, unit, transport_type)
            }
            ModbusResponse::EchoCoil { address, raw_value } => {
                encode_echo_coil(address, raw_value, txn_id, unit, transport_type)
            }
            ModbusResponse::EchoRegister { address, value } => {
                encode_echo_register(address, value, txn_id, unit, transport_type)
            }
            ModbusResponse::EchoMaskWrite {
                address,
                and_mask,
                or_mask,
            } => encode_echo_mask_write(address, and_mask, or_mask, txn_id, unit, transport_type),
            ModbusResponse::EchoMultiWrite { fc, address, count } => {
                encode_echo_multi_write(fc, address, count, txn_id, unit, transport_type)
            }
            #[cfg(feature = "diagnostics")]
            ModbusResponse::SingleByte { fc, value } => {
                encode_single_byte(fc, value, txn_id, unit, transport_type)
            }
            #[cfg(feature = "diagnostics")]
            ModbusResponse::DiagnosticsEcho {
                sub_function,
                result,
            } => encode_diagnostics_echo(sub_function, result, txn_id, unit, transport_type),
            #[cfg(feature = "diagnostics")]
            ModbusResponse::TwoU16 { fc, first, second } => {
                encode_two_u16(fc, first, second, txn_id, unit, transport_type)
            }
            #[cfg(feature = "fifo")]
            ModbusResponse::FifoData { data } => {
                encode_fifo_data(&data, txn_id, unit, transport_type)
            }
            #[cfg(feature = "diagnostics")]
            ModbusResponse::ReadDeviceId {
                read_device_id_code,
                conformity_level,
                more_follows,
                next_object_id,
                objects,
            } => encode_read_device_id(
                read_device_id_code,
                conformity_level,
                more_follows,
                next_object_id,
                &objects,
                txn_id,
                unit,
                transport_type,
            ),
            #[cfg(feature = "file-record")]
            ModbusResponse::FileRecordWriteEcho { pdu_data } => {
                encode_file_record_write_echo(pdu_data, txn_id, unit, transport_type)
            }
            ModbusResponse::Exception { request_fc, code } => {
                encode_exception(request_fc, code, txn_id, unit, transport_type)
            }
            ModbusResponse::ExceptionRaw { fc_byte, code } => {
                encode_exception_raw(fc_byte, code, txn_id, unit, transport_type)
            }
            ModbusResponse::NoResponse => Err(MbusError::Unexpected), // caller must check
        }
    }
}

// ── AsyncTrafficNotifier ─────────────────────────────────────────────────────

/// Optional traffic notifications emitted by the async server session.
///
/// Enabled only when the `traffic` feature is active.  All methods have default
/// no-op implementations — override only the ones you care about.
///
/// This is the async-server counterpart of `mbus_server::TrafficNotifier`.
///
/// # Example
/// ```rust,ignore
/// #[cfg(feature = "traffic")]
/// impl AsyncTrafficNotifier for MyApp {
///     fn on_rx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
///         println!("rx txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
///     }
/// }
/// ```
#[cfg(feature = "traffic")]
pub trait AsyncTrafficNotifier {
    /// Called when an accepted request frame is about to be dispatched to the app.
    ///
    /// Note: `txn_id` is `0` for malformed frames where the header could not be
    /// parsed.  In the normal dispatch path it reflects the actual MBAP transaction ID.
    fn on_rx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {}

    /// Called after a response frame is successfully transmitted.
    fn on_tx_frame(&mut self, _txn_id: u16, _unit: UnitIdOrSlaveAddr, _frame: &[u8]) {}

    /// Called when transmitting a response frame fails.
    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }

    /// Called when an incoming frame cannot be parsed (framing / CRC error).
    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }
}

// ── AsyncAppRequirements ──────────────────────────────────────────────────────

/// Internal super-trait that folds `Send + 'static` (and, when `traffic` is
/// enabled, [`AsyncTrafficNotifier`]) into a single bound used by
/// [`AsyncAppHandler`].
///
/// You never need to implement or name this trait directly.
#[doc(hidden)]
#[cfg(not(feature = "traffic"))]
pub trait AsyncAppRequirements: Send + 'static {}
#[cfg(not(feature = "traffic"))]
impl<T: Send + 'static> AsyncAppRequirements for T {}

#[doc(hidden)]
#[cfg(feature = "traffic")]
pub trait AsyncAppRequirements: AsyncTrafficNotifier + Send + 'static {}
#[cfg(feature = "traffic")]
impl<T: AsyncTrafficNotifier + Send + 'static> AsyncAppRequirements for T {}

// ── AsyncAppHandler ──────────────────────────────────────────────────────────

/// The async application handler trait for the Modbus server.
///
/// # Level 1 — Zero boilerplate
///
/// Use `#[async_modbus_app(...)]` from `mbus-macros` to auto-generate this impl.
/// The macro wires model fields to FC dispatch and optionally calls async hook methods.
///
/// # Traffic notifications
///
/// When the `traffic` feature is enabled, `AsyncAppHandler` requires that the
/// implementing type also implements [`AsyncTrafficNotifier`].  All methods on
/// that trait have default no-op implementations, so a simple `impl
/// AsyncTrafficNotifier for MyApp {}` is enough to satisfy the bound.
///
/// # Send bound
///
/// The future returned by `handle` must be `Send` so sessions can be `tokio::spawn`-ed
/// in multi-threaded runtimes. Implementations that use only `Send`-safe values across
/// `.await` boundaries automatically satisfy this bound.
pub trait AsyncAppHandler: AsyncAppRequirements {
    /// Process a single parsed Modbus request and return the response.
    ///
    /// Called once per received frame by the server session loop. The implementation
    /// may freely `.await` inside this method — database writes, sensor reads, HTTP
    /// calls, etc. are all valid.
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send;

    /// Called whenever the server sends a Modbus exception response.
    ///
    /// Fired for both app-generated exceptions (`handle()` returns
    /// [`ModbusResponse::exception`]) **and** infrastructure-generated ones
    /// (e.g. unknown function code → `IllegalFunction`).
    ///
    /// Default implementation is a no-op.
    fn on_exception(
        &mut self,
        _txn_id: u16,
        _unit: UnitIdOrSlaveAddr,
        _function_code: FunctionCode,
        _exception_code: ExceptionCode,
    ) {
    }
}

/// Blanket impl: `Arc<tokio::sync::Mutex<APP>>` is itself an `AsyncAppHandler`.
///
/// This allows multiple TCP sessions to share one app instance without requiring
/// the user to write forwarding boilerplate.
impl<APP> AsyncAppHandler for std::sync::Arc<tokio::sync::Mutex<APP>>
where
    APP: AsyncAppHandler,
{
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let this = self.clone();
        async move { this.lock().await.handle(req).await }
    }

    fn on_exception(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        exception_code: ExceptionCode,
    ) {
        if let Ok(mut guard) = self.try_lock() {
            guard.on_exception(txn_id, unit, function_code, exception_code);
        }
    }
}

/// When the `traffic` feature is enabled, `Arc<Mutex<APP>>` satisfies
/// [`AsyncTrafficNotifier`] with default no-op hooks.
///
/// Traffic hooks for shared-state apps can be implemented on the inner `APP`
/// type directly or via a custom outer wrapper.  The no-op impl here simply
/// allows `Arc<Mutex<APP>>` to satisfy the `AsyncAppHandler` bound when `traffic`
/// is active without requiring the user to add an extra impl block.
#[cfg(feature = "traffic")]
impl<APP: AsyncAppHandler> AsyncTrafficNotifier for std::sync::Arc<tokio::sync::Mutex<APP>> {}

// ── Private helpers ──────────────────────────────────────────────────────────

// ── Per-variant encode helpers ────────────────────────────────────────────────
//
// Each function encodes exactly one `ModbusResponse` variant into a complete
// ADU frame.  They are free functions so the `encode` match stays as a thin
// dispatcher.

type AduFrame = Vec<u8, MAX_ADU_FRAME_LEN>;
type EncodeResult = Result<AduFrame, MbusError>;

fn encode_byte_count_payload(
    fc: FunctionCode,
    data: &[u8],
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_byte_count_payload(fc, data)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

fn encode_echo_coil(
    address: u16,
    raw_value: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleCoil, address, raw_value)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

fn encode_echo_register(
    address: u16,
    value: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, address, value)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

fn encode_echo_mask_write(
    address: u16,
    and_mask: u16,
    or_mask: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_mask_write_register(address, and_mask, or_mask)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

fn encode_echo_multi_write(
    fc: FunctionCode,
    address: u16,
    count: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_write_single_u16(fc, address, count)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

#[cfg(feature = "diagnostics")]
fn encode_single_byte(
    fc: FunctionCode,
    value: u8,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_byte_payload(fc, value)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

#[cfg(feature = "diagnostics")]
fn encode_diagnostics_echo(
    sub_function: u16,
    result: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_diagnostics(sub_function, result)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

#[cfg(feature = "diagnostics")]
fn encode_two_u16(
    fc: FunctionCode,
    first: u16,
    second: u16,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_write_single_u16(fc, first, second)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

#[cfg(feature = "fifo")]
fn encode_fifo_data(
    data: &[u8],
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let pdu = Pdu::build_fifo_payload(data)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

/// Encodes a `ReadDeviceId` (FC2B / MEI 0x0E) response.
///
/// Builds the 5-byte MEI header (`code`, `conformity`, `more`, `next_id`, `n_objects`),
/// appends the raw object triples, then wraps in a MEI-type PDU.
#[cfg(feature = "diagnostics")]
#[allow(clippy::too_many_arguments)]
fn encode_read_device_id(
    read_device_id_code: u8,
    conformity_level: u8,
    more_follows: bool,
    next_object_id: u8,
    objects: &[u8],
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let n_objects = count_mei_objects(objects).map_err(|_| MbusError::InvalidPduLength)?;
    let more_byte: u8 = if more_follows { 0xFF } else { 0x00 };
    let header = [
        read_device_id_code,
        conformity_level,
        more_byte,
        next_object_id,
        n_objects,
    ];
    if header.len() + objects.len() > MAX_ADU_FRAME_LEN - 1 {
        return Err(MbusError::BufferTooSmall);
    }
    let mut mei_data: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
    mei_data
        .extend_from_slice(&header)
        .map_err(|_| MbusError::BufferTooSmall)?;
    mei_data
        .extend_from_slice(objects)
        .map_err(|_| MbusError::BufferTooSmall)?;
    let pdu = Pdu::build_mei_type(
        FunctionCode::EncapsulatedInterfaceTransport,
        EncapsulatedInterfaceType::ReadDeviceIdentification as u8,
        &mei_data,
    )?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

#[cfg(feature = "file-record")]
fn encode_file_record_write_echo(
    pdu_data: Vec<u8, MAX_PDU_DATA_LEN>,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let len = pdu_data.len() as u8;
    let pdu = Pdu::new(FunctionCode::WriteFileRecord, pdu_data, len);
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

fn encode_exception(
    request_fc: FunctionCode,
    code: ExceptionCode,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let exception_fc = request_fc
        .exception_response()
        .ok_or(MbusError::InvalidFunctionCode)?;
    let pdu = Pdu::build_byte_payload(exception_fc, code as u8)?;
    compile_adu_frame(txn_id, unit.get(), pdu, tt)
}

/// Encode an exception response for an arbitrary (possibly vendor-specific)
/// function-code byte that does not appear in the [`FunctionCode`] enum.
///
/// Follows the Modbus spec: the response function-code byte = `fc_byte | 0x80`.
fn encode_exception_raw(
    fc_byte: u8,
    code: ExceptionCode,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    tt: TransportType,
) -> EncodeResult {
    let exception_fc_byte = fc_byte | 0x80;
    let code_byte = code as u8;
    let unit_id = unit.get();
    let mut frame: AduFrame = AduFrame::new();
    match tt {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // MBAP header: TID(2) + Protocol(2) + Length(2) + UnitID(1) + PDU(2) = 9 bytes
            // Length field = 1 (unit) + 2 (PDU) = 3
            frame.extend_from_slice(&txn_id.to_be_bytes()).map_err(|_| MbusError::Unexpected)?;
            frame.extend_from_slice(&0u16.to_be_bytes()).map_err(|_| MbusError::Unexpected)?;
            frame.extend_from_slice(&3u16.to_be_bytes()).map_err(|_| MbusError::Unexpected)?;
            frame.push(unit_id).map_err(|_| MbusError::Unexpected)?;
            frame.push(exception_fc_byte).map_err(|_| MbusError::Unexpected)?;
            frame.push(code_byte).map_err(|_| MbusError::Unexpected)?;
        }
        TransportType::StdSerial(mode) | TransportType::CustomSerial(mode) => {
            match mode {
                SerialMode::Rtu => {
                    let payload = [unit_id, exception_fc_byte, code_byte];
                    frame.extend_from_slice(&payload).map_err(|_| MbusError::Unexpected)?;
                    let crc = checksum::crc16(&payload);
                    frame.extend_from_slice(&crc.to_le_bytes()).map_err(|_| MbusError::Unexpected)?;
                }
                SerialMode::Ascii => {
                    let binary = [unit_id, exception_fc_byte, code_byte];
                    let lrc = checksum::lrc(&binary);
                    frame.push(b':').map_err(|_| MbusError::Unexpected)?;
                    for &b in &binary {
                        frame.push(raw_nibble_to_hex(b >> 4)).map_err(|_| MbusError::Unexpected)?;
                        frame.push(raw_nibble_to_hex(b & 0x0F)).map_err(|_| MbusError::Unexpected)?;
                    }
                    frame.push(raw_nibble_to_hex(lrc >> 4)).map_err(|_| MbusError::Unexpected)?;
                    frame.push(raw_nibble_to_hex(lrc & 0x0F)).map_err(|_| MbusError::Unexpected)?;
                    frame.push(b'\r').map_err(|_| MbusError::Unexpected)?;
                    frame.push(b'\n').map_err(|_| MbusError::Unexpected)?;
                }
            }
        }
    }
    Ok(frame)
}

#[inline]
fn raw_nibble_to_hex(nibble: u8) -> u8 {
    if nibble < 10 { b'0' + nibble } else { b'A' + nibble - 10 }
}

/// Counts the `[id(1), len(1), value(N)...]` object triples in a FC2B/MEI 0x0E objects payload.
#[cfg(feature = "diagnostics")]
fn count_mei_objects(payload: &[u8]) -> Result<u8, MbusError> {
    let mut offset = 0usize;
    let mut count: u8 = 0;
    while offset < payload.len() {
        if offset + 2 > payload.len() {
            return Err(MbusError::InvalidPduLength);
        }
        let val_len = payload[offset + 1] as usize;
        offset += 2 + val_len;
        if offset > payload.len() {
            return Err(MbusError::InvalidPduLength);
        }
        count = count.checked_add(1).ok_or(MbusError::InvalidPduLength)?;
    }
    Ok(count)
}
