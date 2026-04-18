//! Client response types returned through the task's oneshot channel.
//!
//! [`ClientResponse`] is the typed result enum that the background task sends
//! back to the caller after successfully executing a Modbus request.  Each
//! variant corresponds to one function-code group and is feature-gated to
//! match the corresponding [`ClientRequest`] variants.
//!
//! The `connect()` path uses a plain `Result<(), MbusError>` oneshot directly
//! and is not represented here.
//!
//! [`ClientRequest`]: crate::client::command::ClientRequest

#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::DeviceIdentificationResponse;
#[cfg(feature = "discrete-inputs")]
use mbus_core::models::discrete_input::DiscreteInputs;
#[cfg(feature = "fifo")]
use mbus_core::models::fifo_queue::FifoQueue;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::SubRequestParams;
#[cfg(feature = "registers")]
use mbus_core::models::register::Registers;

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};

#[cfg(feature = "diagnostics")]
use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::MAX_SUB_REQUESTS_PER_PDU;

// ─── ClientResponse ───────────────────────────────────────────────────────────

/// Typed response payload returned to the async caller via a oneshot channel.
///
/// The task encodes the response variant that matches the outgoing request type.
/// [`AsyncClientCore`] pattern-matches the variant to extract the concrete return
/// type expected by each public method.
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
#[derive(Debug)]
pub(crate) enum ClientResponse {
    // ── Coils ────────────────────────────────────────────────────────────
    /// Read Coils (FC 01), Write Single Coil (FC 05), Write Multiple Coils (FC 0F).
    #[cfg(feature = "coils")]
    Coils(Coils),

    // ── Registers ────────────────────────────────────────────────────────
    /// Read Holding Registers (FC 03), Read Input Registers (FC 04),
    /// Write Multiple Registers (FC 10), Read/Write Multiple Registers (FC 17).
    #[cfg(feature = "registers")]
    Registers(Registers),
    /// Write Single Register (FC 06) echo-back confirmation.
    #[cfg(feature = "registers")]
    SingleRegisterWrite {
        /// Echoed register address.
        address: u16,
        /// Echoed written value.
        value: u16,
    },
    /// Mask Write Register (FC 16) acknowledgement.
    #[cfg(feature = "registers")]
    MaskWriteRegister,

    // ── Discrete inputs ───────────────────────────────────────────────────
    /// Read Discrete Inputs (FC 02).
    #[cfg(feature = "discrete-inputs")]
    DiscreteInputs(DiscreteInputs),

    // ── FIFO queue ────────────────────────────────────────────────────────
    /// Read FIFO Queue (FC 18).
    #[cfg(feature = "fifo")]
    FifoQueue(FifoQueue),

    // ── File record ───────────────────────────────────────────────────────
    /// Read File Record (FC 14) — parsed sub-request results.
    #[cfg(feature = "file-record")]
    FileRecordRead(heapless::Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>),
    /// Write File Record (FC 15) acknowledgement.
    #[cfg(feature = "file-record")]
    FileRecordWrite,

    // ── Diagnostics ───────────────────────────────────────────────────────
    /// Read Device Identification (FC 43 / MEI 0E).
    #[cfg(feature = "diagnostics")]
    DeviceIdentification(DeviceIdentificationResponse),
    /// Encapsulated Interface Transport (FC 43 / non-0E MEI type).
    #[cfg(feature = "diagnostics")]
    EncapsulatedInterfaceTransport {
        /// MEI type code from the response.
        mei_type: EncapsulatedInterfaceType,
        /// Raw response bytes.
        data: heapless::Vec<u8, MAX_PDU_DATA_LEN>,
    },
    /// Read Exception Status (FC 07).
    #[cfg(feature = "diagnostics")]
    ExceptionStatus(u8),
    /// Diagnostics (FC 08) echo-back.
    #[cfg(feature = "diagnostics")]
    DiagnosticsData {
        /// Echoed sub-function code.
        sub_function: DiagnosticSubFunction,
        /// Echoed data words.
        data: heapless::Vec<u16, MAX_PDU_DATA_LEN>,
    },
    /// Get Comm Event Counter (FC 0B).
    #[cfg(feature = "diagnostics")]
    CommEventCounter {
        /// Device communication status word.
        status: u16,
        /// Number of successfully completed events since last restart.
        event_count: u16,
    },
    /// Get Comm Event Log (FC 0C).
    #[cfg(feature = "diagnostics")]
    CommEventLog {
        /// Device communication status word.
        status: u16,
        /// Total events recorded.
        event_count: u16,
        /// Total messages processed.
        message_count: u16,
        /// Raw event bytes (oldest first).
        events: heapless::Vec<u8, MAX_PDU_DATA_LEN>,
    },
    /// Report Server ID (FC 11).
    #[cfg(feature = "diagnostics")]
    ReportServerId(heapless::Vec<u8, MAX_PDU_DATA_LEN>),
}
