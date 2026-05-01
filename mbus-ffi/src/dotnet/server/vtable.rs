//! .NET vtable-based dispatch adapter implementing [`AsyncAppHandler`].
//!
//! # Design
//!
//! The C# side supplies function pointers via [`MbusDnServerVtable`].  Each
//! optional slot corresponds to one Modbus function code.  When a request
//! arrives, the [`DotNetServerAdapter`] calls the matching slot if it is
//! present; if the slot is `None` it returns an `IllegalFunction` exception.
//!
//! ## Return values from vtable callbacks
//!
//! All callbacks return an `i32`:
//! * `0` — success; response data has been written to the out-parameter
//!   buffers.
//! * positive — Modbus exception code (`1` = IllegalFunction,
//!   `2` = IllegalDataAddress, `3` = IllegalDataValue, etc.).
//! * negative — server-device failure (maps to
//!   `ServerDeviceFailure` exception).
//!
//! ## Buffer sizes
//!
//! Read callbacks receive stack buffers of `VTABLE_BUF_WORDS` (128) words or
//! `VTABLE_BUF_BYTES` (256) bytes.  The callback must not write beyond the
//! supplied length.

use core::ffi::c_void;
use std::future::Future;
use std::sync::Arc;

use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_server_async::{AsyncAppHandler, ModbusRequest, ModbusResponse};

/// Maximum byte buffer size passed to vtable read callbacks.
pub const VTABLE_BUF_BYTES: usize = 256;
/// Maximum word (u16) buffer size passed to vtable read callbacks.
pub const VTABLE_BUF_WORDS: usize = 128;

// ── Vtable struct ─────────────────────────────────────────────────────────────

/// C-compatible vtable of optional Modbus request handler callbacks.
///
/// Set any slot to `None` (`null` from C#) to return an `IllegalFunction`
/// exception for that function code.  All function pointers receive `ctx` as
/// their first argument.
///
/// # Repr
///
/// The struct uses `#[repr(C)]` so that it has a stable, platform-defined
/// layout.  Pass it by pointer to `mbus_dn_tcp_server_new`.
#[repr(C)]
pub struct MbusDnServerVtable {
    /// Caller-supplied opaque context forwarded unchanged to every callback.
    pub ctx: *mut c_void,

    // ── FC01 — Read Coils ────────────────────────────────────────────────────
    /// `fn(ctx, address, count, out_packed_bytes, out_byte_count) -> i32`
    #[cfg(feature = "coils")]
    pub read_coils: Option<unsafe extern "C" fn(*mut c_void, u16, u16, *mut u8, *mut u16) -> i32>,

    // ── FC05 — Write Single Coil ─────────────────────────────────────────────
    /// `fn(ctx, address, value_bool) -> i32`
    #[cfg(feature = "coils")]
    pub write_single_coil: Option<unsafe extern "C" fn(*mut c_void, u16, u8) -> i32>,

    // ── FC0F — Write Multiple Coils ──────────────────────────────────────────
    /// `fn(ctx, address, packed_bytes, byte_count, coil_count) -> i32`
    #[cfg(feature = "coils")]
    pub write_multiple_coils:
        Option<unsafe extern "C" fn(*mut c_void, u16, *const u8, u16, u16) -> i32>,

    // ── FC02 — Read Discrete Inputs ──────────────────────────────────────────
    /// `fn(ctx, address, count, out_packed_bytes, out_byte_count) -> i32`
    #[cfg(feature = "discrete-inputs")]
    pub read_discrete_inputs:
        Option<unsafe extern "C" fn(*mut c_void, u16, u16, *mut u8, *mut u16) -> i32>,

    // ── FC03 — Read Holding Registers ────────────────────────────────────────
    /// `fn(ctx, address, count, out_u16_values, out_count) -> i32`
    #[cfg(feature = "registers")]
    pub read_holding_registers:
        Option<unsafe extern "C" fn(*mut c_void, u16, u16, *mut u16, *mut u16) -> i32>,

    // ── FC04 — Read Input Registers ──────────────────────────────────────────
    /// `fn(ctx, address, count, out_u16_values, out_count) -> i32`
    #[cfg(feature = "registers")]
    pub read_input_registers:
        Option<unsafe extern "C" fn(*mut c_void, u16, u16, *mut u16, *mut u16) -> i32>,

    // ── FC06 — Write Single Register ─────────────────────────────────────────
    /// `fn(ctx, address, value) -> i32`
    #[cfg(feature = "registers")]
    pub write_single_register: Option<unsafe extern "C" fn(*mut c_void, u16, u16) -> i32>,

    // ── FC10 — Write Multiple Registers ─────────────────────────────────────
    /// `fn(ctx, address, values_be_bytes, count) -> i32`
    #[cfg(feature = "registers")]
    pub write_multiple_registers:
        Option<unsafe extern "C" fn(*mut c_void, u16, *const u8, u16) -> i32>,

    // ── FC22 — Mask Write Register ───────────────────────────────────────────
    /// `fn(ctx, address, and_mask, or_mask) -> i32`
    #[cfg(feature = "registers")]
    pub mask_write_register: Option<unsafe extern "C" fn(*mut c_void, u16, u16, u16) -> i32>,

    // ── FC23 — Read/Write Multiple Registers ─────────────────────────────────
    /// `fn(ctx, read_addr, read_count, write_addr, write_values_be_bytes,
    ///    write_count, out_u16_values, out_count) -> i32`
    #[cfg(feature = "registers")]
    pub read_write_multiple_registers: Option<
        unsafe extern "C" fn(
            *mut c_void,
            u16,
            u16,
            u16,
            *const u8,
            u16,
            *mut u16,
            *mut u16,
        ) -> i32,
    >,

    // ── FC24 — Read FIFO Queue ───────────────────────────────────────────────
    /// `fn(ctx, pointer_address, out_u16_values, out_count) -> i32`
    #[cfg(feature = "fifo")]
    pub read_fifo_queue:
        Option<unsafe extern "C" fn(*mut c_void, u16, *mut u16, *mut u16) -> i32>,

    // ── FC07 — Read Exception Status ─────────────────────────────────────────
    /// `fn(ctx, out_status_byte) -> i32`
    #[cfg(feature = "diagnostics")]
    pub read_exception_status: Option<unsafe extern "C" fn(*mut c_void, *mut u8) -> i32>,

    // ── FC08 — Diagnostics ───────────────────────────────────────────────────
    /// `fn(ctx, sub_fn, data, out_sub_fn, out_data) -> i32`
    #[cfg(feature = "diagnostics")]
    pub diagnostics:
        Option<unsafe extern "C" fn(*mut c_void, u16, u16, *mut u16, *mut u16) -> i32>,

    // ── FC0B — Get Comm Event Counter ────────────────────────────────────────
    /// `fn(ctx, out_status_word, out_event_count) -> i32`
    #[cfg(feature = "diagnostics")]
    pub get_comm_event_counter:
        Option<unsafe extern "C" fn(*mut c_void, *mut u16, *mut u16) -> i32>,

    // ── FC0C — Get Comm Event Log ────────────────────────────────────────────
    /// `fn(ctx, out_payload_bytes, out_byte_count) -> i32`
    ///
    /// The payload must be formatted as:
    /// `[status_hi, status_lo, event_count_hi, event_count_lo,
    ///   msg_count_hi, msg_count_lo, event_bytes...]`
    #[cfg(feature = "diagnostics")]
    pub get_comm_event_log: Option<unsafe extern "C" fn(*mut c_void, *mut u8, *mut u16) -> i32>,

    // ── FC11 — Report Server ID ──────────────────────────────────────────────
    /// `fn(ctx, out_payload_bytes, out_byte_count) -> i32`
    #[cfg(feature = "diagnostics")]
    pub report_server_id: Option<unsafe extern "C" fn(*mut c_void, *mut u8, *mut u16) -> i32>,
}

// The vtable contains raw function pointers and a `*mut c_void` context.
// The C# caller is responsible for keeping the context alive and for
// ensuring that the callbacks are safe to call from any Tokio worker thread.
unsafe impl Send for MbusDnServerVtable {}
unsafe impl Sync for MbusDnServerVtable {}

// ── Adapter ───────────────────────────────────────────────────────────────────

/// Rust adapter that implements [`AsyncAppHandler`] by delegating to a
/// [`MbusDnServerVtable`] supplied from C#.
#[derive(Clone)]
pub struct DotNetServerAdapter {
    pub(super) vtable: Arc<MbusDnServerVtable>,
}

impl DotNetServerAdapter {
    pub fn new(vtable: MbusDnServerVtable) -> Self {
        Self {
            vtable: Arc::new(vtable),
        }
    }

    pub fn new_with_arc(vtable: Arc<MbusDnServerVtable>) -> Self {
        Self { vtable }
    }
}

#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncTrafficNotifier for DotNetServerAdapter {}

impl AsyncAppHandler for DotNetServerAdapter {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let vt = self.vtable.clone();
        async move { dispatch(&vt, req) }
    }
}

// ── dispatch ─────────────────────────────────────────────────────────────────

fn exception_from_i32(fc: FunctionCode, code: i32) -> ModbusResponse {
    let ex = match code {
        1 => ExceptionCode::IllegalFunction,
        2 => ExceptionCode::IllegalDataAddress,
        3 => ExceptionCode::IllegalDataValue,
        _ => ExceptionCode::ServerDeviceFailure,
    };
    ModbusResponse::exception(fc, ex)
}

fn exception_raw_from_i32(fc_byte: u8, code: i32) -> ModbusResponse {
    let ex = match code {
        1 => ExceptionCode::IllegalFunction,
        2 => ExceptionCode::IllegalDataAddress,
        3 => ExceptionCode::IllegalDataValue,
        _ => ExceptionCode::ServerDeviceFailure,
    };
    ModbusResponse::exception_raw(fc_byte, ex)
}

fn dispatch(vt: &MbusDnServerVtable, req: ModbusRequest) -> ModbusResponse {
    match req {
        // ── FC01 — Read Coils ────────────────────────────────────────────────
        #[cfg(feature = "coils")]
        ModbusRequest::ReadCoils { address, count, .. } => {
            let Some(f) = vt.read_coils else {
                return ModbusResponse::exception(
                    FunctionCode::ReadCoils,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u8; VTABLE_BUF_BYTES];
            let mut written: u16 = 0;
            let rc =
                unsafe { f(vt.ctx, address, count, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadCoils, rc);
            }
            ModbusResponse::packed_bits(
                FunctionCode::ReadCoils,
                &buf[..written as usize],
            )
        }

        // ── FC05 — Write Single Coil ─────────────────────────────────────────
        #[cfg(feature = "coils")]
        ModbusRequest::WriteSingleCoil { address, value, .. } => {
            let Some(f) = vt.write_single_coil else {
                return ModbusResponse::exception(
                    FunctionCode::WriteSingleCoil,
                    ExceptionCode::IllegalFunction,
                );
            };
            let rc = unsafe { f(vt.ctx, address, value as u8) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::WriteSingleCoil, rc);
            }
            ModbusResponse::echo_coil(address, value)
        }

        // ── FC0F — Write Multiple Coils ──────────────────────────────────────
        #[cfg(feature = "coils")]
        ModbusRequest::WriteMultipleCoils {
            address,
            count,
            data,
            ..
        } => {
            let Some(f) = vt.write_multiple_coils else {
                return ModbusResponse::exception(
                    FunctionCode::WriteMultipleCoils,
                    ExceptionCode::IllegalFunction,
                );
            };
            let rc = unsafe {
                f(
                    vt.ctx,
                    address,
                    data.as_ptr(),
                    data.len() as u16,
                    count,
                )
            };
            if rc != 0 {
                return exception_from_i32(FunctionCode::WriteMultipleCoils, rc);
            }
            ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleCoils, address, count)
        }

        // ── FC02 — Read Discrete Inputs ──────────────────────────────────────
        #[cfg(feature = "discrete-inputs")]
        ModbusRequest::ReadDiscreteInputs { address, count, .. } => {
            let Some(f) = vt.read_discrete_inputs else {
                return ModbusResponse::exception(
                    FunctionCode::ReadDiscreteInputs,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u8; VTABLE_BUF_BYTES];
            let mut written: u16 = 0;
            let rc =
                unsafe { f(vt.ctx, address, count, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadDiscreteInputs, rc);
            }
            ModbusResponse::packed_bits(
                FunctionCode::ReadDiscreteInputs,
                &buf[..written as usize],
            )
        }

        // ── FC03 — Read Holding Registers ────────────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::ReadHoldingRegisters { address, count, .. } => {
            let Some(f) = vt.read_holding_registers else {
                return ModbusResponse::exception(
                    FunctionCode::ReadHoldingRegisters,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u16; VTABLE_BUF_WORDS];
            let mut written: u16 = 0;
            let rc = unsafe { f(vt.ctx, address, count, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadHoldingRegisters, rc);
            }
            ModbusResponse::registers(
                FunctionCode::ReadHoldingRegisters,
                &buf[..written as usize],
            )
        }

        // ── FC04 — Read Input Registers ──────────────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::ReadInputRegisters { address, count, .. } => {
            let Some(f) = vt.read_input_registers else {
                return ModbusResponse::exception(
                    FunctionCode::ReadInputRegisters,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u16; VTABLE_BUF_WORDS];
            let mut written: u16 = 0;
            let rc = unsafe { f(vt.ctx, address, count, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadInputRegisters, rc);
            }
            ModbusResponse::registers(
                FunctionCode::ReadInputRegisters,
                &buf[..written as usize],
            )
        }

        // ── FC06 — Write Single Register ─────────────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::WriteSingleRegister { address, value, .. } => {
            let Some(f) = vt.write_single_register else {
                return ModbusResponse::exception(
                    FunctionCode::WriteSingleRegister,
                    ExceptionCode::IllegalFunction,
                );
            };
            let rc = unsafe { f(vt.ctx, address, value) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::WriteSingleRegister, rc);
            }
            ModbusResponse::echo_register(address, value)
        }

        // ── FC10 — Write Multiple Registers ──────────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::WriteMultipleRegisters {
            address,
            count,
            data,
            ..
        } => {
            let Some(f) = vt.write_multiple_registers else {
                return ModbusResponse::exception(
                    FunctionCode::WriteMultipleRegisters,
                    ExceptionCode::IllegalFunction,
                );
            };
            let rc = unsafe { f(vt.ctx, address, data.as_ptr(), count) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::WriteMultipleRegisters, rc);
            }
            ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleRegisters, address, count)
        }

        // ── FC22 — Mask Write Register ───────────────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::MaskWriteRegister {
            address,
            and_mask,
            or_mask,
            ..
        } => {
            let Some(f) = vt.mask_write_register else {
                return ModbusResponse::exception(
                    FunctionCode::MaskWriteRegister,
                    ExceptionCode::IllegalFunction,
                );
            };
            let rc = unsafe { f(vt.ctx, address, and_mask, or_mask) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::MaskWriteRegister, rc);
            }
            ModbusResponse::echo_mask_write(address, and_mask, or_mask)
        }

        // ── FC23 — Read/Write Multiple Registers ──────────────────────────────
        #[cfg(feature = "registers")]
        ModbusRequest::ReadWriteMultipleRegisters {
            read_address,
            read_count,
            write_address,
            write_count,
            data,
            ..
        } => {
            let Some(f) = vt.read_write_multiple_registers else {
                return ModbusResponse::exception(
                    FunctionCode::ReadWriteMultipleRegisters,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u16; VTABLE_BUF_WORDS];
            let mut written: u16 = 0;
            let rc = unsafe {
                f(
                    vt.ctx,
                    read_address,
                    read_count,
                    write_address,
                    data.as_ptr(),
                    write_count,
                    buf.as_mut_ptr(),
                    &mut written,
                )
            };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadWriteMultipleRegisters, rc);
            }
            ModbusResponse::registers(
                FunctionCode::ReadWriteMultipleRegisters,
                &buf[..written as usize],
            )
        }

        // ── FC24 — Read FIFO Queue ───────────────────────────────────────────
        #[cfg(feature = "fifo")]
        ModbusRequest::ReadFifoQueue {
            pointer_address, ..
        } => {
            let Some(f) = vt.read_fifo_queue else {
                return ModbusResponse::exception(
                    FunctionCode::ReadFifoQueue,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u16; VTABLE_BUF_WORDS];
            let mut written: u16 = 0;
            let rc = unsafe { f(vt.ctx, pointer_address, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadFifoQueue, rc);
            }
            // Encode FIFO payload: fifo_count (2 BE) + values (2 BE each)
            let count = written as usize;
            let mut payload = heapless::Vec::<u8, { mbus_core::data_unit::common::MAX_ADU_FRAME_LEN }>::new();
            let _ = payload.extend_from_slice(&(count as u16).to_be_bytes());
            for v in &buf[..count] {
                let _ = payload.extend_from_slice(&v.to_be_bytes());
            }
            ModbusResponse::fifo_response(&payload)
        }

        // ── FC07 — Read Exception Status ─────────────────────────────────────
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReadExceptionStatus { .. } => {
            let Some(f) = vt.read_exception_status else {
                return ModbusResponse::exception(
                    FunctionCode::ReadExceptionStatus,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut status: u8 = 0;
            let rc = unsafe { f(vt.ctx, &mut status) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReadExceptionStatus, rc);
            }
            ModbusResponse::read_exception_status(status)
        }

        // ── FC08 — Diagnostics ───────────────────────────────────────────────
        #[cfg(feature = "diagnostics")]
        ModbusRequest::Diagnostics {
            sub_function, data, ..
        } => {
            let Some(f) = vt.diagnostics else {
                // default: echo
                return ModbusResponse::diagnostics_echo(sub_function, data);
            };
            let mut out_sub: u16 = sub_function;
            let mut out_data: u16 = data;
            let rc = unsafe { f(vt.ctx, sub_function, data, &mut out_sub, &mut out_data) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::Diagnostics, rc);
            }
            ModbusResponse::diagnostics_echo(out_sub, out_data)
        }

        // ── FC0B — Get Comm Event Counter ────────────────────────────────────
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventCounter { .. } => {
            let Some(f) = vt.get_comm_event_counter else {
                return ModbusResponse::exception(
                    FunctionCode::GetCommEventCounter,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut status_word: u16 = 0;
            let mut event_count: u16 = 0;
            let rc = unsafe { f(vt.ctx, &mut status_word, &mut event_count) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::GetCommEventCounter, rc);
            }
            ModbusResponse::comm_event_counter(status_word, event_count)
        }

        // ── FC0C — Get Comm Event Log ────────────────────────────────────────
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventLog { .. } => {
            let Some(f) = vt.get_comm_event_log else {
                return ModbusResponse::exception(
                    FunctionCode::GetCommEventLog,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u8; VTABLE_BUF_BYTES];
            let mut written: u16 = 0;
            let rc = unsafe { f(vt.ctx, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::GetCommEventLog, rc);
            }
            ModbusResponse::comm_event_log(&buf[..written as usize])
        }

        // ── FC11 — Report Server ID ──────────────────────────────────────────
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReportServerId { .. } => {
            let Some(f) = vt.report_server_id else {
                return ModbusResponse::exception(
                    FunctionCode::ReportServerId,
                    ExceptionCode::IllegalFunction,
                );
            };
            let mut buf = [0u8; VTABLE_BUF_BYTES];
            let mut written: u16 = 0;
            let rc = unsafe { f(vt.ctx, buf.as_mut_ptr(), &mut written) };
            if rc != 0 {
                return exception_from_i32(FunctionCode::ReportServerId, rc);
            }
            ModbusResponse::report_server_id(&buf[..written as usize])
        }

        // ── Unhandled — return IllegalFunction ───────────────────────────────
        other => exception_raw_from_i32(other.function_code_byte(), 1),
    }
}
