//! Client command types for the async task channel.
//!
//! This module defines the two enums used to communicate with the background
//! `tokio` task that owns the transport:
//!
//! - [`ClientRequest`] — the user-facing operation parameters, one variant per
//!   Modbus function code, carrying only the values the caller supplies.  The
//!   task assigns the transaction id internally.
//! - [`TaskCommand`] — the full envelope sent over the `mpsc` channel, which
//!   wraps a [`ClientRequest`] together with the oneshot reply sender, or
//!   represents a connection request.
//!
//! Shutdown is signalled implicitly by dropping the `mpsc::Sender` end of the
//! channel; no explicit `Shutdown` variant is needed.

use tokio::sync::oneshot;

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::SubRequest;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};

use crate::client::response::ClientResponse;

/// Reply channel used to deliver a single response back to the async caller.
pub(crate) type ResponseSender = oneshot::Sender<Result<ClientResponse, MbusError>>;

// ─── ClientRequest ────────────────────────────────────────────────────────────

/// User-supplied parameters for a single Modbus request.
///
/// Each variant corresponds to one function code (or a closely related group).
/// The task assigns the transaction id; callers never supply it here.
#[derive(Debug)]
pub(crate) enum ClientRequest {
    // ── Coils (FC 01 / 05 / 0F) ───────────────────────────────────────────
    #[cfg(feature = "coils")]
    ReadMultipleCoils {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    },
    #[cfg(feature = "coils")]
    WriteSingleCoil {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    },
    #[cfg(feature = "coils")]
    WriteMultipleCoils {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        coils: Coils,
    },

    // ── Registers (FC 03 / 04 / 06 / 10 / 16 / 17) ────────────────────────
    #[cfg(feature = "registers")]
    ReadHoldingRegisters {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    },
    #[cfg(feature = "registers")]
    ReadInputRegisters {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    },
    #[cfg(feature = "registers")]
    WriteSingleRegister {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    },
    #[cfg(feature = "registers")]
    WriteMultipleRegisters {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        values: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    #[cfg(feature = "registers")]
    ReadWriteMultipleRegisters {
        unit: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    #[cfg(feature = "registers")]
    MaskWriteRegister {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    },

    // ── Discrete inputs (FC 02) ────────────────────────────────────────────
    #[cfg(feature = "discrete-inputs")]
    ReadDiscreteInputs {
        unit: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    },

    // ── FIFO queue (FC 18) ─────────────────────────────────────────────────
    #[cfg(feature = "fifo")]
    ReadFifoQueue {
        unit: UnitIdOrSlaveAddr,
        address: u16,
    },

    // ── File record (FC 14 / 15) ───────────────────────────────────────────
    #[cfg(feature = "file-record")]
    ReadFileRecord {
        unit: UnitIdOrSlaveAddr,
        sub_request: SubRequest,
    },
    #[cfg(feature = "file-record")]
    WriteFileRecord {
        unit: UnitIdOrSlaveAddr,
        sub_request: SubRequest,
    },

    // ── Diagnostics (FC 07 / 08 / 0B / 0C / 11 / 2B) ─────────────────────
    #[cfg(feature = "diagnostics")]
    ReadDeviceIdentification {
        unit: UnitIdOrSlaveAddr,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    },
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        unit: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: heapless::Vec<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    #[cfg(feature = "diagnostics")]
    ReadExceptionStatus {
        unit: UnitIdOrSlaveAddr,
    },
    #[cfg(feature = "diagnostics")]
    Diagnostics {
        unit: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: heapless::Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>,
    },
    #[cfg(feature = "diagnostics")]
    GetCommEventCounter {
        unit: UnitIdOrSlaveAddr,
    },
    #[cfg(feature = "diagnostics")]
    GetCommEventLog {
        unit: UnitIdOrSlaveAddr,
    },
    #[cfg(feature = "diagnostics")]
    ReportServerId {
        unit: UnitIdOrSlaveAddr,
    },
}

impl ClientRequest {
    /// Returns the target unit id or slave address for this request.
    pub(crate) fn unit(&self) -> UnitIdOrSlaveAddr {
        match self {
            #[cfg(feature = "coils")]
            Self::ReadMultipleCoils { unit, .. }
            | Self::WriteSingleCoil { unit, .. }
            | Self::WriteMultipleCoils { unit, .. } => *unit,

            #[cfg(feature = "registers")]
            Self::ReadHoldingRegisters { unit, .. }
            | Self::ReadInputRegisters { unit, .. }
            | Self::WriteSingleRegister { unit, .. }
            | Self::WriteMultipleRegisters { unit, .. }
            | Self::ReadWriteMultipleRegisters { unit, .. }
            | Self::MaskWriteRegister { unit, .. } => *unit,

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
        }
    }
}

// ─── TaskCommand ──────────────────────────────────────────────────────────────

/// Command envelope sent from [`AsyncClientCore`] to the background task.
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
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
