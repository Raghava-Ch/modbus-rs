//! Client command types for the async task channel.
//!
//! This module defines the two enums used to communicate with the background
//! `tokio` task that owns the transport:
//!
//! - [`ClientRequest`] — the user-facing operation parameters, one variant per
//!   Modbus function code, carrying only the values the caller supplies.  The
//!   task assigns the transaction id internally.
//! - `TaskCommand` — the full envelope sent over the `mpsc` channel, which
//!   wraps a [`ClientRequest`] together with the oneshot reply sender, or
//!   represents a connection request.
//!
//! Shutdown is signalled implicitly by dropping the `mpsc::Sender` end of the
//! channel; no explicit `Shutdown` variant is needed.

use tokio::sync::oneshot;

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::SubRequest;

use crate::client::response::ClientResponse;

/// Reply channel used to deliver a single response back to the async caller.
pub(crate) type ResponseSender = oneshot::Sender<Result<ClientResponse, MbusError>>;

// ─── ClientRequest ────────────────────────────────────────────────────────────

/// User-supplied parameters for a single Modbus request.
///
/// Each variant corresponds to one function code (or a closely related group).
/// The task assigns the transaction id; callers never supply it here.
#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
pub enum ClientRequest {
    // ── Coils (FC 01 / 05 / 0F) ───────────────────────────────────────────
    /// Read multiple coils (FC 01).
    #[cfg(feature = "coils")]
    ReadMultipleCoils {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of coils to read.
        quantity: u16,
    },
    /// Write a single coil (FC 05).
    #[cfg(feature = "coils")]
    WriteSingleCoil {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Address to write.
        address: u16,
        /// Value to write.
        value: bool,
    },
    /// Write multiple coils (FC 15 / 0F).
    #[cfg(feature = "coils")]
    WriteMultipleCoils {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Coils values to write.
        coils: Coils,
    },

    // ── Registers (FC 03 / 04 / 06 / 10 / 16 / 17) ────────────────────────
    /// Read holding registers (FC 03).
    #[cfg(feature = "holding-registers")]
    ReadHoldingRegisters {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of registers to read.
        quantity: u16,
    },
    /// Read input registers (FC 04).
    #[cfg(feature = "input-registers")]
    ReadInputRegisters {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of registers to read.
        quantity: u16,
    },
    /// Write a single register (FC 06).
    #[cfg(feature = "holding-registers")]
    WriteSingleRegister {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Address to write.
        address: u16,
        /// Value to write.
        value: u16,
    },
    /// Write multiple registers (FC 16 / 10).
    #[cfg(feature = "holding-registers")]
    WriteMultipleRegisters {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Register values to write.
        values: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    /// Read and write multiple registers (FC 23 / 17).
    #[cfg(feature = "holding-registers")]
    ReadWriteMultipleRegisters {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Address to read from.
        read_address: u16,
        /// Number of registers to read.
        read_quantity: u16,
        /// Address to write to.
        write_address: u16,
        /// Register values to write.
        write_values: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    /// Mask write register (FC 22 / 16).
    #[cfg(feature = "holding-registers")]
    MaskWriteRegister {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Address to write.
        address: u16,
        /// AND mask to apply.
        and_mask: u16,
        /// OR mask to apply.
        or_mask: u16,
    },

    // ── Discrete inputs (FC 02) ────────────────────────────────────────────
    /// Read discrete inputs (FC 02).
    #[cfg(feature = "discrete-inputs")]
    ReadDiscreteInputs {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Starting address.
        address: u16,
        /// Number of inputs to read.
        quantity: u16,
    },

    // ── FIFO queue (FC 18) ─────────────────────────────────────────────────
    /// Read FIFO queue (FC 18).
    #[cfg(feature = "fifo")]
    ReadFifoQueue {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// FIFO queue address.
        address: u16,
    },

    // ── File record (FC 14 / 15) ───────────────────────────────────────────
    /// Read file record (FC 14).
    #[cfg(feature = "file-record")]
    ReadFileRecord {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Sub request details.
        sub_request: SubRequest,
    },
    /// Write file record (FC 15).
    #[cfg(feature = "file-record")]
    WriteFileRecord {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Sub request details.
        sub_request: SubRequest,
    },

    // ── Diagnostics (FC 07 / 08 / 0B / 0C / 11 / 2B) ─────────────────────
    /// Read device identification (FC 43/14).
    #[cfg(feature = "diagnostics")]
    ReadDeviceIdentification {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Read Device ID Code.
        read_device_id_code: ReadDeviceIdCode,
        /// Object ID to request.
        object_id: ObjectId,
    },
    /// Encapsulated interface transport (FC 43).
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// MEI Type.
        mei_type: EncapsulatedInterfaceType,
        /// MEI request data.
        data: heapless::Vec<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    /// Read exception status (FC 07).
    #[cfg(feature = "diagnostics")]
    ReadExceptionStatus {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
    },
    /// Diagnostics sub-function execution (FC 08).
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
        /// Diagnostic sub-function code.
        sub_function: DiagnosticSubFunction,
        /// Sub-function data.
        data: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    /// Get communication event counter (FC 11 / 0B).
    #[cfg(feature = "diagnostics")]
    GetCommEventCounter {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
    },
    /// Get communication event log (FC 12 / 0C).
    #[cfg(feature = "diagnostics")]
    GetCommEventLog {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
    },
    /// Report server ID (FC 17 / 11).
    #[cfg(feature = "diagnostics")]
    ReportServerId {
        /// Unit or slave address of the Modbus device.
        unit: UnitIdOrSlaveAddr,
    },
}

impl ClientRequest {
    /// Returns the target unit id or slave address for this request.
    #[allow(dead_code)]
    pub(crate) fn unit(&self) -> UnitIdOrSlaveAddr {
        match self {
            #[cfg(feature = "coils")]
            Self::ReadMultipleCoils { unit, .. }
            | Self::WriteSingleCoil { unit, .. }
            | Self::WriteMultipleCoils { unit, .. } => *unit,

            #[cfg(feature = "holding-registers")]
            Self::ReadHoldingRegisters { unit, .. }
            | Self::WriteSingleRegister { unit, .. }
            | Self::WriteMultipleRegisters { unit, .. }
            | Self::ReadWriteMultipleRegisters { unit, .. }
            | Self::MaskWriteRegister { unit, .. } => *unit,

            #[cfg(feature = "input-registers")]
            Self::ReadInputRegisters { unit, .. } => *unit,

            #[cfg(feature = "discrete-inputs")]
            Self::ReadDiscreteInputs { unit, .. } => *unit,

            #[cfg(feature = "fifo")]
            Self::ReadFifoQueue { unit, .. } => *unit,

            #[cfg(feature = "file-record")]
            Self::ReadFileRecord { unit, .. } | Self::WriteFileRecord { unit, .. } => *unit,

            #[cfg(feature = "diagnostics")]
            Self::ReadDeviceIdentification { unit, .. }
            | Self::EncapsulatedInterfaceTransport { unit, .. }
            | Self::ReadExceptionStatus { unit, .. }
            | Self::Diagnostics { unit, .. }
            | Self::GetCommEventCounter { unit, .. }
            | Self::GetCommEventLog { unit, .. }
            | Self::ReportServerId { unit, .. } => *unit,

            #[allow(unreachable_patterns)]
            _ => unreachable!(),
        }
    }
}

// ─── TaskCommand ──────────────────────────────────────────────────────────────

/// Command envelope sent from [`AsyncClientCore`] to the background task.
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
// `ClientRequest` is 18 KiB on the stack; the enum is transient (sent once over
// the mpsc channel and immediately consumed), so boxing is unnecessary overhead.
#[allow(clippy::large_enum_variant)]
pub(crate) enum TaskCommand {
    /// Establish the transport connection.
    Connect {
        resp_tx: oneshot::Sender<Result<(), MbusError>>,
    },
    /// Execute a Modbus function-code request.
    Request {
        params: ClientRequest,
        resp_tx: ResponseSender,
    },
    /// Drain all in-flight and queued requests with `ConnectionClosed` and
    /// close the transport.  Issued automatically after a per-request timeout
    /// so the pipeline self-heals without caller intervention.
    Disconnect,
}
