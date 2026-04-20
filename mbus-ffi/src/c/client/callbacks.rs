use core::ffi::c_void;

use super::error::MbusStatusCode;

// ── Opaque Model Types ───────────────────────────────────────────────────────

#[cfg(feature = "coils")]
use super::models::coils::MbusCoils;
#[cfg(feature = "discrete-inputs")]
use super::models::discrete_inputs::MbusDiscreteInputs;
#[cfg(feature = "fifo")]
use super::models::fifo::MbusFifoQueue;
#[cfg(feature = "registers")]
use super::models::registers::MbusRegisters;

// ── File-record response view type ───────────────────────────────────────────

/// A single sub-request result returned in the `on_read_file_record` callback.
///
/// `data` points into Rust-owned memory and is **valid only during the callback
/// invocation**. Copy the slice if you need to retain it.
#[cfg(feature = "file-record")]
#[repr(C)]
pub struct MbusFileRecordResult {
    /// Record number for this sub-request.
    pub record_number: u16,
    /// Pointer to the register data for this sub-request chunk.
    pub data: *const u16,
    /// Number of u16 words in `data`.
    pub data_len: u16,
}

// ── Callback context structs ─────────────────────────────────────────────────

// ── Coil callbacks (feature "coils") ─────────────────────────────────────────

#[cfg(feature = "coils")]
#[repr(C)]
/// Context passed to the read-coils callback.
pub struct MbusReadCoilsCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to the coils data (valid during this callback only).
    pub coils: *const MbusCoils,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "coils")]
#[repr(C)]
/// Context passed to the write-single-coil callback.
pub struct MbusWriteSingleCoilCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Coil address.
    pub address: u16,
    /// Written value (1 = ON, 0 = OFF).
    pub value: u8,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "coils")]
#[repr(C)]
/// Context passed to the write-multiple-coils callback.
pub struct MbusWriteMultipleCoilsCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Starting coil address.
    pub address: u16,
    /// Number of coils written.
    pub quantity: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── Register callbacks (feature "registers") ─────────────────────────────────

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the read-holding-registers callback.
pub struct MbusReadHoldingRegistersCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to registers data.
    pub registers: *const MbusRegisters,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the read-input-registers callback.
pub struct MbusReadInputRegistersCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to registers data.
    pub registers: *const MbusRegisters,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the read-write-multiple-registers callback.
pub struct MbusReadWriteMultipleRegistersCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to registers data.
    pub registers: *const MbusRegisters,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the write-single-register callback.
pub struct MbusWriteSingleRegisterCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Register address.
    pub address: u16,
    /// Written value.
    pub value: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the write-multiple-registers callback.
pub struct MbusWriteMultipleRegistersCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Starting register address.
    pub address: u16,
    /// Number of registers written.
    pub quantity: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "registers")]
#[repr(C)]
/// Context passed to the mask-write-register callback.
pub struct MbusMaskWriteRegisterCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── Discrete input callbacks (feature "discrete-inputs") ──────────────────────

#[cfg(feature = "discrete-inputs")]
#[repr(C)]
/// Context passed to the read-discrete-inputs callback.
pub struct MbusReadDiscreteInputsCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to discrete inputs data.
    pub discrete_inputs: *const MbusDiscreteInputs,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── FIFO callbacks (feature "fifo") ───────────────────────────────────────────

#[cfg(feature = "fifo")]
#[repr(C)]
/// Context passed to the read-fifo-queue callback.
pub struct MbusReadFifoQueueCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Opaque pointer to FIFO queue data.
    pub fifo_queue: *const MbusFifoQueue,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── File-record callbacks (feature "file-record") ─────────────────────────────

#[cfg(feature = "file-record")]
#[repr(C)]
/// Context passed to the read-file-record callback.
pub struct MbusReadFileRecordCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Pointer to array of sub-request results.
    pub results: *const MbusFileRecordResult,
    /// Number of sub-request entries in `results`.
    pub count: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "file-record")]
#[repr(C)]
/// Context passed to the write-file-record callback.
pub struct MbusWriteFileRecordCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── Diagnostics callbacks (feature "diagnostics") ────────────────────────────

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the read-exception-status callback.
pub struct MbusReadExceptionStatusCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Exception status byte.
    pub status: u8,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the diagnostics callback.
pub struct MbusDiagnosticsCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Diagnostic sub-function code.
    pub sub_fn: u16,
    /// Pointer to diagnostic data words.
    pub data: *const u16,
    /// Number of u16 words in `data`.
    pub data_len: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the comm-event-counter callback.
pub struct MbusCommEventCounterCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Status word.
    pub status: u16,
    /// Event count.
    pub event_count: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the comm-event-log callback.
pub struct MbusCommEventLogCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Status word.
    pub status: u16,
    /// Event count.
    pub event_count: u16,
    /// Message count.
    pub message_count: u16,
    /// Pointer to event bytes.
    pub events: *const u8,
    /// Number of bytes in `events`.
    pub events_len: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the report-server-id callback.
pub struct MbusReportServerIdCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Server ID byte.
    pub server_id: u8,
    /// Run indicator (0xFF = ON, 0x00 = OFF).
    pub run_indicator: u8,
    /// Pointer to additional device identifier data.
    pub device_identifier: *const u8,
    /// Length of `device_identifier` data.
    pub identifier_len: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

#[cfg(feature = "diagnostics")]
#[repr(C)]
/// Context passed to the read-device-id callback.
pub struct MbusReadDeviceIdCtx {
    /// Transaction ID.
    pub txn_id: u16,
    /// Unit / slave ID.
    pub unit_id: u8,
    /// Read Device ID code that was requested.
    pub read_device_id_code: u8,
    /// Conformity level reported by the device.
    pub conformity_level: u8,
    /// 1 if more objects follow, 0 if this is the last.
    pub more_follows: u8,
    /// Pointer to raw MEI response bytes.
    pub objects: *const u8,
    /// Number of bytes in `objects`.
    pub objects_len: u16,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── Error callback ────────────────────────────────────────────────────────────

/// Context passed to the request-failed callback.
#[repr(C)]
pub struct MbusRequestFailedCtx {
    /// Transaction ID of the failed request.
    pub txn_id: u16,
    /// Unit / slave ID the request was targeting.
    pub unit_id: u8,
    /// Error code describing the failure.
    pub error: MbusStatusCode,
    /// User-provided opaque pointer.
    pub userdata: *mut c_void,
}

// ── Time callback ─────────────────────────────────────────────────────────────

/// Called by Rust whenever current time in milliseconds is needed.
///
/// This is required for no_std compatibility: the C host must provide time.
pub type MbusCurrentMillisCb = unsafe extern "C" fn(userdata: *mut c_void) -> u64;

// ── MbusCallbacks ─────────────────────────────────────────────────────────────

/// Table of C function-pointer callbacks.
///
/// Set a field to `NULL` to ignore that response type. All non-null callbacks
/// will be invoked synchronously from within `mbus_tcp_poll()` /
/// `mbus_serial_poll()`.
///
/// `userdata` is passed verbatim to every callback invocation; use it to
/// thread your own application context through.
#[repr(C)]
pub struct MbusCallbacks {
    /// Opaque application context pointer threaded through every callback.
    pub userdata: *mut c_void,

    /// Required callback used for all timekeeping in the client state machine.
    ///
    /// If this callback is NULL, client construction fails.
    pub on_current_millis: Option<unsafe extern "C" fn(userdata: *mut c_void) -> u64>,

    // ── Coil callbacks (feature "coils") ──────────────────────────────────
    #[cfg(feature = "coils")]
    /// Called when a Read Coils response is received.
    pub on_read_coils: Option<unsafe extern "C" fn(ctx: *const MbusReadCoilsCtx)>,
    #[cfg(feature = "coils")]
    /// Called when a Write Single Coil response is received.
    pub on_write_single_coil: Option<unsafe extern "C" fn(ctx: *const MbusWriteSingleCoilCtx)>,
    #[cfg(feature = "coils")]
    /// Called when a Write Multiple Coils response is received.
    pub on_write_multiple_coils:
        Option<unsafe extern "C" fn(ctx: *const MbusWriteMultipleCoilsCtx)>,

    // ── Register callbacks (feature "registers") ──────────────────────────
    #[cfg(feature = "registers")]
    /// Called when a Read Holding Registers response is received.
    pub on_read_holding_registers:
        Option<unsafe extern "C" fn(ctx: *const MbusReadHoldingRegistersCtx)>,
    #[cfg(feature = "registers")]
    /// Called when a Read Input Registers response is received.
    pub on_read_input_registers:
        Option<unsafe extern "C" fn(ctx: *const MbusReadInputRegistersCtx)>,
    #[cfg(feature = "registers")]
    /// Called when a Read/Write Multiple Registers response is received.
    pub on_read_write_multiple_registers:
        Option<unsafe extern "C" fn(ctx: *const MbusReadWriteMultipleRegistersCtx)>,
    #[cfg(feature = "registers")]
    /// Called when a Write Single Register response is received.
    pub on_write_single_register:
        Option<unsafe extern "C" fn(ctx: *const MbusWriteSingleRegisterCtx)>,
    #[cfg(feature = "registers")]
    /// Called when a Write Multiple Registers response is received.
    pub on_write_multiple_registers:
        Option<unsafe extern "C" fn(ctx: *const MbusWriteMultipleRegistersCtx)>,
    #[cfg(feature = "registers")]
    /// Called when a Mask Write Register response is received.
    pub on_mask_write_register: Option<unsafe extern "C" fn(ctx: *const MbusMaskWriteRegisterCtx)>,

    // ── Discrete input callbacks (feature "discrete-inputs") ──────────────
    #[cfg(feature = "discrete-inputs")]
    /// Called when a Read Discrete Inputs response is received.
    pub on_read_discrete_inputs:
        Option<unsafe extern "C" fn(ctx: *const MbusReadDiscreteInputsCtx)>,

    // ── FIFO callbacks (feature "fifo") ───────────────────────────────────
    #[cfg(feature = "fifo")]
    /// Called when a Read FIFO Queue response is received.
    pub on_read_fifo_queue: Option<unsafe extern "C" fn(ctx: *const MbusReadFifoQueueCtx)>,

    // ── File-record callbacks (feature "file-record") ─────────────────────
    #[cfg(feature = "file-record")]
    /// Called when a Read File Record response is received.
    pub on_read_file_record: Option<unsafe extern "C" fn(ctx: *const MbusReadFileRecordCtx)>,
    #[cfg(feature = "file-record")]
    /// Called when a Write File Record response is received.
    pub on_write_file_record: Option<unsafe extern "C" fn(ctx: *const MbusWriteFileRecordCtx)>,

    // ── Diagnostics callbacks (feature "diagnostics") ─────────────────────
    #[cfg(feature = "diagnostics")]
    /// Called when a Read Exception Status response is received.
    pub on_read_exception_status:
        Option<unsafe extern "C" fn(ctx: *const MbusReadExceptionStatusCtx)>,
    #[cfg(feature = "diagnostics")]
    /// Called when a Diagnostics response is received.
    pub on_diagnostics: Option<unsafe extern "C" fn(ctx: *const MbusDiagnosticsCtx)>,
    #[cfg(feature = "diagnostics")]
    /// Called when a Comm Event Counter response is received.
    pub on_comm_event_counter: Option<unsafe extern "C" fn(ctx: *const MbusCommEventCounterCtx)>,
    #[cfg(feature = "diagnostics")]
    /// Called when a Comm Event Log response is received.
    pub on_comm_event_log: Option<unsafe extern "C" fn(ctx: *const MbusCommEventLogCtx)>,
    #[cfg(feature = "diagnostics")]
    /// Called when a Report Server ID response is received.
    pub on_report_server_id: Option<unsafe extern "C" fn(ctx: *const MbusReportServerIdCtx)>,
    #[cfg(feature = "diagnostics")]
    /// Called when a Read Device Identification response is received.
    pub on_read_device_id: Option<unsafe extern "C" fn(ctx: *const MbusReadDeviceIdCtx)>,

    // ── Error callback ─────────────────────────────────────────────────────
    /// Called when a request fails (timeout, exception, etc.).
    pub on_request_failed: Option<unsafe extern "C" fn(ctx: *const MbusRequestFailedCtx)>,
}
