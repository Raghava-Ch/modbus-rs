//! C-facing callback types for the Modbus server FFI.
//!
//! ## Design Principles
//!
//! - One `*Req` struct per Modbus function code (or FC group member).
//! - **Input fields** come from the master request (parsed by the Rust server stack).
//! - **Output fields** (`out_*`) are pre-populated by Rust before the callback is
//!   invoked; C writes the response values directly into them.
//! - Output buffer fields (`out_data`, `out_events`, `out_server_id`) point **into
//!   Rust-owned stack buffers** that are valid only for the duration of the callback.
//!   C **MUST NOT** retain these pointers after the callback returns.
//! - C returns [`MbusServerExceptionCode`] to signal success or a Modbus exception.
//!   `Ok(0)` means the request was handled; any other value causes the server to
//!   send a Modbus exception response to the client.
//!
//! ## Safety
//!
//! All `*mut`/`*const` pointer fields in request structs are guaranteed non-null by
//! the Rust layer when a callback is invoked. C code should not pass NULL for these
//! fields when populating response outputs.
//!
//! ## Lifetime of output pointers
//!
//! ```text
//! Rust calls callback ──► C writes to out_data ──► Rust reads out_byte_count ──► done
//!                     ↑                                                       ↑
//!                     │                            Pointer is valid only here │
//! ```

use core::ffi::c_void;

// ── Exception codes ───────────────────────────────────────────────────────────

/// Exception code returned by every C server callback.
///
/// Maps 1-to-1 with the Modbus standard exception codes:
/// - `0x01` IllegalFunction
/// - `0x02` IllegalDataAddress
/// - `0x03` IllegalDataValue
/// - `0x04` ServerDeviceFailure
///
/// The value `Ok = 0` is a non-standard sentinel used by this API to signal
/// "request handled successfully; no exception".
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbusServerExceptionCode {
    /// No exception — the callback handled the request successfully.
    Ok = 0,
    /// FC is not supported / not implemented by this server.
    IllegalFunction = 1,
    /// Address or offset is outside the server's valid range.
    IllegalDataAddress = 2,
    /// A data value in the request is not acceptable.
    IllegalDataValue = 3,
    /// The server could not complete the request due to an internal failure.
    ServerDeviceFailure = 4,
}

// ── Per-FC request structs ────────────────────────────────────────────────────
//
// Naming convention:
//   MbusServer<FunctionName>Req     — flattened request + pre-allocated output fields.
//
// Output fields:
//   - Scalar outputs  : plain value fields (e.g. `out_byte_count: u8`). C writes the value.
//   - Buffer outputs  : `out_data: *mut u8, out_data_len: usize`. C writes into the supplied
//                       Rust buffer (at most `out_data_len` bytes) then sets the `out_byte_count`
//                       field to how many bytes were written.
//   - Pointer outputs : `*const` slice fields (e.g. for write FCs) hold immutable data from
//                       the master request. C reads them and must not retain them.

// ── FC01 / FC02 / FC03 / FC04 — Read handlers with packed byte output ─────────

/// Request context for FC 0x01 — Read Coils.
///
/// C reads `address` and `quantity`; writes packed coil bits into `out_data` (at most
/// `out_data_len` bytes) and sets `out_byte_count` to the number of bytes written.
#[repr(C)]
pub struct MbusServerReadCoilsReq {
    /// Slave/unit address of the request.
    pub unit_id: u8,
    /// Transaction ID (MBAP) or 0 for serial.
    pub txn_id: u16,
    /// Starting coil address.
    pub address: u16,
    /// Number of coils to read.
    pub quantity: u16,
    /// Buffer to write packed coil bytes into (Rust-allocated, valid for callback duration).
    pub out_data: *mut u8,
    /// Capacity of `out_data` in bytes.
    pub out_data_len: usize,
    /// C must set this to the number of bytes written into `out_data`.
    pub out_byte_count: u8,
}

/// C callback type for FC 0x01.
pub type MbusServerReadCoilsFn = Option<
    unsafe extern "C" fn(req: *mut MbusServerReadCoilsReq, userdata: *mut c_void)
        -> MbusServerExceptionCode,
>;

/// Request context for FC 0x02 — Read Discrete Inputs.
#[repr(C)]
pub struct MbusServerReadDiscreteInputsReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    pub quantity: u16,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    pub out_byte_count: u8,
}

/// C callback type for FC 0x02.
pub type MbusServerReadDiscreteInputsFn = Option<
    unsafe extern "C" fn(req: *mut MbusServerReadDiscreteInputsReq, userdata: *mut c_void)
        -> MbusServerExceptionCode,
>;

/// Request context for FC 0x03 — Read Holding Registers.
///
/// `out_data` receives big-endian register bytes (2 bytes per register).
/// `out_byte_count` must be set to `quantity * 2` on success.
#[repr(C)]
pub struct MbusServerReadHoldingRegistersReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    pub quantity: u16,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    pub out_byte_count: u8,
}

/// C callback type for FC 0x03.
pub type MbusServerReadHoldingRegistersFn = Option<
    unsafe extern "C" fn(req: *mut MbusServerReadHoldingRegistersReq, userdata: *mut c_void)
        -> MbusServerExceptionCode,
>;

/// Request context for FC 0x04 — Read Input Registers.
#[repr(C)]
pub struct MbusServerReadInputRegistersReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    pub quantity: u16,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    pub out_byte_count: u8,
}

/// C callback type for FC 0x04.
pub type MbusServerReadInputRegistersFn = Option<
    unsafe extern "C" fn(req: *mut MbusServerReadInputRegistersReq, userdata: *mut c_void)
        -> MbusServerExceptionCode,
>;

// ── FC05 / FC06 — Write single (no output data) ───────────────────────────────

/// Request context for FC 0x05 — Write Single Coil.
#[repr(C)]
pub struct MbusServerWriteSingleCoilReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    /// `true` = force coil ON, `false` = force coil OFF.
    pub value: bool,
}

/// C callback type for FC 0x05.
pub type MbusServerWriteSingleCoilFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerWriteSingleCoilReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

/// Request context for FC 0x06 — Write Single Register.
#[repr(C)]
pub struct MbusServerWriteSingleRegisterReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    pub value: u16,
}

/// C callback type for FC 0x06.
pub type MbusServerWriteSingleRegisterFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerWriteSingleRegisterReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC07 — Read Exception Status ─────────────────────────────────────────────

/// Request context for FC 0x07 — Read Exception Status.
///
/// C sets `out_status` to the 8-bit exception status byte.
#[repr(C)]
pub struct MbusServerReadExceptionStatusReq {
    pub unit_id: u8,
    pub txn_id: u16,
    /// C must set this to the 8-bit exception status byte.
    pub out_status: u8,
}

/// C callback type for FC 0x07.
pub type MbusServerReadExceptionStatusFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReadExceptionStatusReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC08 — Diagnostics ───────────────────────────────────────────────────────

/// Request context for FC 0x08 — Diagnostics.
///
/// `sub_function` is the raw 16-bit sub-function code (see Modbus spec §6.8).
/// C sets `out_result` to the 16-bit data echo/result.
#[repr(C)]
pub struct MbusServerDiagnosticsReq {
    pub unit_id: u8,
    pub txn_id: u16,
    /// Raw sub-function code (e.g. 0x0000 = Return Query Data, 0x0002 = Return Diagnostic Register).
    pub sub_function: u16,
    /// Data word from the request.
    pub data: u16,
    /// C must set this to the 16-bit result/echo value.
    pub out_result: u16,
}

/// C callback type for FC 0x08.
pub type MbusServerDiagnosticsFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerDiagnosticsReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC0B — Get Comm Event Counter ─────────────────────────────────────────────

/// Request context for FC 0x0B — Get Comm Event Counter.
///
/// C sets `out_status` (status word) and `out_event_count`.
#[repr(C)]
pub struct MbusServerGetCommEventCounterReq {
    pub unit_id: u8,
    pub txn_id: u16,
    /// C must set this to the status word.
    pub out_status: u16,
    /// C must set this to the event counter value.
    pub out_event_count: u16,
}

/// C callback type for FC 0x0B.
pub type MbusServerGetCommEventCounterFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerGetCommEventCounterReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC0C — Get Comm Event Log ─────────────────────────────────────────────────

/// Request context for FC 0x0C — Get Comm Event Log.
///
/// C writes event bytes into `out_events` (max `out_events_len` bytes) and
/// sets the four output scalar fields.
#[repr(C)]
pub struct MbusServerGetCommEventLogReq {
    pub unit_id: u8,
    pub txn_id: u16,
    /// Buffer for event log bytes (Rust-allocated, valid for callback duration).
    pub out_events: *mut u8,
    /// Capacity of `out_events`.
    pub out_events_len: usize,
    /// C must set to the Status word.
    pub out_status: u16,
    /// C must set to the Event Count.
    pub out_event_count: u16,
    /// C must set to the Message Count.
    pub out_message_count: u16,
    /// C must set to the number of events written into `out_events`.
    pub out_num_events: u8,
}

/// C callback type for FC 0x0C.
pub type MbusServerGetCommEventLogFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerGetCommEventLogReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC0F — Write Multiple Coils ───────────────────────────────────────────────

/// Request context for FC 0x0F — Write Multiple Coils.
///
/// `values` is a packed bit array from the master (LSB of first byte = coil at
/// `starting_address`). Valid for the duration of the callback only.
#[repr(C)]
pub struct MbusServerWriteMultipleCoilsReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub starting_address: u16,
    pub quantity: u16,
    /// Packed coil bits from the master (read-only, valid for callback duration).
    pub values: *const u8,
    /// Number of bytes in `values`.
    pub values_len: usize,
}

/// C callback type for FC 0x0F.
pub type MbusServerWriteMultipleCoilsFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerWriteMultipleCoilsReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC10 — Write Multiple Registers ──────────────────────────────────────────

/// Request context for FC 0x10 — Write Multiple Registers.
///
/// `values` contains the decoded `u16` register values (big-endian already parsed).
/// Valid for the duration of the callback only.
#[repr(C)]
pub struct MbusServerWriteMultipleRegistersReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub starting_address: u16,
    /// Decoded register values from the master (read-only, valid for callback duration).
    pub values: *const u16,
    /// Number of elements in `values`.
    pub values_len: usize,
}

/// C callback type for FC 0x10.
pub type MbusServerWriteMultipleRegistersFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerWriteMultipleRegistersReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC11 — Report Server ID ───────────────────────────────────────────────────

/// Request context for FC 0x11 — Report Server ID.
///
/// C writes vendor/device identification bytes into `out_server_id` and sets
/// `out_byte_count` and `out_run_indicator_status`.
#[repr(C)]
pub struct MbusServerReportServerIdReq {
    pub unit_id: u8,
    pub txn_id: u16,
    /// Buffer for server ID bytes (Rust-allocated).
    pub out_server_id: *mut u8,
    /// Capacity of `out_server_id`.
    pub out_server_id_len: usize,
    /// C must set to the number of Server ID bytes written.
    pub out_byte_count: u8,
    /// C must set to the Run Indicator Status byte (0x00 = OFF, 0xFF = ON).
    pub out_run_indicator_status: u8,
}

/// C callback type for FC 0x11.
pub type MbusServerReportServerIdFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReportServerIdReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC14 — Read File Record ───────────────────────────────────────────────────

/// Request context for FC 0x14 — Read File Record (one sub-request at a time).
///
/// C writes record data bytes into `out_data` and sets `out_byte_count`.
#[repr(C)]
pub struct MbusServerReadFileRecordReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub file_number: u16,
    pub record_number: u16,
    pub record_length: u16,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    /// C must set to the number of bytes written into `out_data`.
    pub out_byte_count: u8,
}

/// C callback type for FC 0x14.
pub type MbusServerReadFileRecordFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReadFileRecordReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC15 — Write File Record ──────────────────────────────────────────────────

/// Request context for FC 0x15 — Write File Record (one sub-request at a time).
///
/// `record_data` contains decoded `u16` words. Valid for the callback duration only.
#[repr(C)]
pub struct MbusServerWriteFileRecordReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub file_number: u16,
    pub record_number: u16,
    pub record_length: u16,
    /// Decoded record data words (read-only, valid for callback duration).
    pub record_data: *const u16,
    pub record_data_len: usize,
}

/// C callback type for FC 0x15.
pub type MbusServerWriteFileRecordFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerWriteFileRecordReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC16 — Mask Write Register ────────────────────────────────────────────────

/// Request context for FC 0x16 — Mask Write Register.
#[repr(C)]
pub struct MbusServerMaskWriteRegisterReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub address: u16,
    pub and_mask: u16,
    pub or_mask: u16,
}

/// C callback type for FC 0x16.
pub type MbusServerMaskWriteRegisterFn = Option<
    unsafe extern "C" fn(
        req: *const MbusServerMaskWriteRegisterReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC17 — Read/Write Multiple Registers ──────────────────────────────────────

/// Request context for FC 0x17 — Read/Write Multiple Registers.
///
/// C writes the read result into `out_data` (big-endian register bytes) and sets
/// `out_byte_count`. `write_values` contains the decoded write values from the master.
#[repr(C)]
pub struct MbusServerReadWriteMultipleRegistersReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub read_address: u16,
    pub read_quantity: u16,
    pub write_address: u16,
    /// Decoded write values from the master (read-only, valid for callback duration).
    pub write_values: *const u16,
    pub write_values_len: usize,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    /// C must set to the number of bytes written into `out_data`.
    pub out_byte_count: u8,
}

/// C callback type for FC 0x17.
pub type MbusServerReadWriteMultipleRegistersFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReadWriteMultipleRegistersReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC18 — Read FIFO Queue ────────────────────────────────────────────────────

/// Request context for FC 0x18 — Read FIFO Queue.
#[repr(C)]
pub struct MbusServerReadFifoQueueReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub pointer_address: u16,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    /// C must set to the number of bytes written.
    pub out_byte_count: u8,
}

/// C callback type for FC 0x18.
pub type MbusServerReadFifoQueueFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReadFifoQueueReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── FC2B/0x0E — Read Device Identification ────────────────────────────────────

/// Request context for FC 0x2B / MEI 0x0E — Read Device Identification.
///
/// C writes object data into `out_data` and sets all `out_*` fields.
#[repr(C)]
pub struct MbusServerReadDeviceIdentificationReq {
    pub unit_id: u8,
    pub txn_id: u16,
    pub read_device_id_code: u8,
    pub start_object_id: u8,
    pub out_data: *mut u8,
    pub out_data_len: usize,
    /// C must set to the conformity level byte.
    pub out_conformity_level: u8,
    /// C must set to the "more follows" object ID (0x00 if none).
    pub out_more_follows_object_id: u8,
    /// C must set to `true` if more objects follow (segmentation).
    pub out_has_more: bool,
    /// C must set to the next Object ID to request (only valid when `out_has_more` is true).
    pub out_next_object_id: u8,
    /// C must set to the number of bytes written into `out_data`.
    pub out_byte_count: u8,
}

/// C callback type for FC 0x2B / MEI 0x0E.
pub type MbusServerReadDeviceIdentificationFn = Option<
    unsafe extern "C" fn(
        req: *mut MbusServerReadDeviceIdentificationReq,
        userdata: *mut c_void,
    ) -> MbusServerExceptionCode,
>;

// ── MbusServerHandlers ────────────────────────────────────────────────────────

/// Master callback table for a C Modbus server application.
///
/// Pass a populated instance of this struct to `mbus_tcp_server_new` or
/// `mbus_serial_server_new`. Any callback left as `NULL` will cause the server
/// to respond with `ExceptionCode::IllegalFunction` for that function code.
///
/// The `userdata` pointer is passed as-is to every callback. It is the caller's
/// responsibility to ensure its lifetime exceeds the server's.
///
/// # Example (C)
///
/// ```c
/// static MbusServerHandlers g_handlers = {
///     .userdata         = &my_app_state,
///     .on_read_coils    = my_read_coils_handler,
///     .on_write_single_coil = my_write_single_coil_handler,
///     // leave other fields NULL to return IllegalFunction
/// };
/// ```
#[repr(C)]
pub struct MbusServerHandlers {
    /// Opaque pointer passed to every callback.
    pub userdata: *mut c_void,

    // ── Coil handlers (FC01, FC05, FC0F) ─────────────────────────────────────
    pub on_read_coils: MbusServerReadCoilsFn,
    pub on_write_single_coil: MbusServerWriteSingleCoilFn,
    pub on_write_multiple_coils: MbusServerWriteMultipleCoilsFn,

    // ── Discrete Input handlers (FC02) ────────────────────────────────────────
    pub on_read_discrete_inputs: MbusServerReadDiscreteInputsFn,

    // ── Holding Register handlers (FC03, FC06, FC10, FC16, FC17) ─────────────
    pub on_read_holding_registers: MbusServerReadHoldingRegistersFn,
    pub on_write_single_register: MbusServerWriteSingleRegisterFn,
    pub on_write_multiple_registers: MbusServerWriteMultipleRegistersFn,
    pub on_mask_write_register: MbusServerMaskWriteRegisterFn,
    pub on_read_write_multiple_registers: MbusServerReadWriteMultipleRegistersFn,

    // ── Input Register handlers (FC04) ────────────────────────────────────────
    pub on_read_input_registers: MbusServerReadInputRegistersFn,

    // ── FIFO Queue handlers (FC18) ────────────────────────────────────────────
    pub on_read_fifo_queue: MbusServerReadFifoQueueFn,

    // ── File Record handlers (FC14, FC15) ─────────────────────────────────────
    pub on_read_file_record: MbusServerReadFileRecordFn,
    pub on_write_file_record: MbusServerWriteFileRecordFn,

    // ── Diagnostics handlers (FC07, FC08, FC0B, FC0C, FC11, FC2B) ────────────
    pub on_read_exception_status: MbusServerReadExceptionStatusFn,
    pub on_diagnostics: MbusServerDiagnosticsFn,
    pub on_get_comm_event_counter: MbusServerGetCommEventCounterFn,
    pub on_get_comm_event_log: MbusServerGetCommEventLogFn,
    pub on_report_server_id: MbusServerReportServerIdFn,
    pub on_read_device_identification: MbusServerReadDeviceIdentificationFn,
}
