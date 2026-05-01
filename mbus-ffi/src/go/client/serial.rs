//! Go cgo surface for [`mbus_client_async::AsyncSerialClient`].
//!
//! Mirrors `tcp.rs` but wraps the serial (RTU / ASCII) client.  Two
//! constructors are provided — `mbus_go_serial_client_new_rtu` and
//! `mbus_go_serial_client_new_ascii` — that accept plain C strings for the
//! port path.  All request entry points are identical to the TCP variants
//! in calling convention and semantics.

use core::ffi::c_char;
use core::ptr;
use core::slice;
use std::ffi::CStr;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use mbus_client_async::AsyncSerialClient;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "file-record")]
use {
    heapless::Vec as HVec,
    mbus_client_async::SubRequest,
    mbus_core::data_unit::common::MAX_PDU_DATA_LEN,
};
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
};

use crate::go::runtime;
use crate::go::status::{self, MbusGoStatus};

/// Opaque handle to an asynchronous Modbus serial client (RTU or ASCII).
#[allow(missing_docs)]
pub struct MbusGoSerialClient {
    inner: Arc<AsyncSerialClient>,
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

fn make_serial_client(
    port: *const c_char,
    baud_rate: u32,
    data_bits_val: u8,
    parity_val: u8,
    stop_bits: u8,
    response_timeout_ms: u32,
    mode: SerialMode,
) -> Option<AsyncSerialClient> {
    if port.is_null() {
        return None;
    }
    let port_str = unsafe { CStr::from_ptr(port) }.to_str().ok()?;
    let port_path = heapless::String::<64>::from_str(port_str).ok()?;

    let baud_rate = match baud_rate {
        9600 => BaudRate::Baud9600,
        19200 => BaudRate::Baud19200,
        other => BaudRate::Custom(other),
    };
    let data_bits = match data_bits_val {
        5 => DataBits::Five,
        6 => DataBits::Six,
        7 => DataBits::Seven,
        8 => DataBits::Eight,
        _ => return None,
    };
    let parity = match parity_val {
        0 => Parity::None,
        1 => Parity::Even,
        2 => Parity::Odd,
        _ => return None,
    };
    if stop_bits != 1 && stop_bits != 2 {
        return None;
    }

    let config = ModbusSerialConfig {
        port_path,
        mode,
        baud_rate,
        data_bits,
        stop_bits,
        parity,
        response_timeout_ms,
        retry_attempts: 0,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };

    let rt = runtime::get();
    let _guard = rt.enter();
    match mode {
        SerialMode::Rtu => AsyncSerialClient::new_rtu(config).ok(),
        SerialMode::Ascii => AsyncSerialClient::new_ascii(config).ok(),
    }
}

/// Creates a new RTU serial client.
///
/// `port` must be a NUL-terminated UTF-8 path string (e.g. `"/dev/ttyUSB0"` on
/// Linux or `"COM3"` on Windows).  `parity` is `0`=None, `1`=Even, `2`=Odd.
/// Returns `null` on any configuration or construction failure.
///
/// # Safety
///
/// `port` must point to a valid NUL-terminated string for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_new_rtu(
    port: *const c_char,
    baud_rate: u32,
    data_bits: u8,
    parity: u8,
    stop_bits: u8,
    response_timeout_ms: u32,
) -> *mut MbusGoSerialClient {
    match make_serial_client(
        port,
        baud_rate,
        data_bits,
        parity,
        stop_bits,
        response_timeout_ms,
        SerialMode::Rtu,
    ) {
        Some(client) => Box::into_raw(Box::new(MbusGoSerialClient {
            inner: Arc::new(client),
        })),
        None => ptr::null_mut(),
    }
}

/// Creates a new ASCII serial client.
///
/// See [`mbus_go_serial_client_new_rtu`] for parameter semantics.
///
/// # Safety
///
/// `port` must point to a valid NUL-terminated string for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_new_ascii(
    port: *const c_char,
    baud_rate: u32,
    data_bits: u8,
    parity: u8,
    stop_bits: u8,
    response_timeout_ms: u32,
) -> *mut MbusGoSerialClient {
    match make_serial_client(
        port,
        baud_rate,
        data_bits,
        parity,
        stop_bits,
        response_timeout_ms,
        SerialMode::Ascii,
    ) {
        Some(client) => Box::into_raw(Box::new(MbusGoSerialClient {
            inner: Arc::new(client),
        })),
        None => ptr::null_mut(),
    }
}

/// Destroys the serial client and releases all associated resources.
///
/// # Safety
///
/// `handle` must be a valid pointer originally returned by one of the
/// `mbus_go_serial_client_new_*` constructors.  Calling this function more
/// than once with the same pointer is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_free(handle: *mut MbusGoSerialClient) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

// ── Connection management ────────────────────────────────────────────────────

/// Opens the underlying serial port.
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_connect(
    handle: *mut MbusGoSerialClient,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.connect()) {
        Ok(()) => MbusGoStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}

/// Closes the underlying serial port.
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_disconnect(
    handle: *mut MbusGoSerialClient,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.disconnect()) {
        Ok(()) => MbusGoStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}

/// Sets the per-request timeout in milliseconds (`0` = no timeout).
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_set_request_timeout_ms(
    handle: *mut MbusGoSerialClient,
    timeout_ms: u64,
) {
    if let Some(h) = unsafe { handle.as_ref() } {
        if timeout_ms == 0 {
            h.inner.clear_request_timeout();
        } else {
            h.inner
                .set_request_timeout(Duration::from_millis(timeout_ms));
        }
    }
}

// ── Register FCs ─────────────────────────────────────────────────────────────

/// Reads `quantity` holding registers (FC03).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_holding_registers(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    if out_buf_len < quantity {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let rt = runtime::get();
    let regs = match rt.block_on(client.read_holding_registers(unit_id, address, quantity)) {
        Ok(r) => r,
        Err(e) => return status::from_async(e),
    };
    let qty = regs.quantity();
    let base = regs.from_address();
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, qty as usize) };
    for (i, slot) in dst.iter_mut().enumerate() {
        *slot = regs.value(base + i as u16).unwrap_or(0);
    }
    unsafe { *out_count = qty };
    MbusGoStatus::MbusOk
}

/// Writes a single holding register (FC06).
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_write_single_register(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    value: u16,
    out_address: *mut u16,
    out_value: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.write_single_register(unit_id, address, value)) {
        Ok((addr, val)) => {
            if !out_address.is_null() {
                unsafe { *out_address = addr };
            }
            if !out_value.is_null() {
                unsafe { *out_value = val };
            }
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Writes multiple holding registers (FC16).
///
/// `values` is a slice of `quantity` big-endian register words.
///
/// # Safety
///
/// `handle` and `values` must be valid non-null pointers.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_write_multiple_registers(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    values: *const u16,
    quantity: u16,
    out_address: *mut u16,
    out_quantity: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if values.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let vals = unsafe { slice::from_raw_parts(values, quantity as usize) };
    let rt = runtime::get();
    match rt.block_on(client.write_multiple_registers(unit_id, address, vals)) {
        Ok((addr, qty)) => {
            if !out_address.is_null() {
                unsafe { *out_address = addr };
            }
            if !out_quantity.is_null() {
                unsafe { *out_quantity = qty };
            }
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reads `quantity` input registers (FC04).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_input_registers(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    if out_buf_len < quantity {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let rt = runtime::get();
    let regs = match rt.block_on(client.read_input_registers(unit_id, address, quantity)) {
        Ok(r) => r,
        Err(e) => return status::from_async(e),
    };
    let qty = regs.quantity();
    let base = regs.from_address();
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, qty as usize) };
    for (i, slot) in dst.iter_mut().enumerate() {
        *slot = regs.value(base + i as u16).unwrap_or(0);
    }
    unsafe { *out_count = qty };
    MbusGoStatus::MbusOk
}

/// Applies an AND/OR bitmask to a holding register (FC22).
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_mask_write_register(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.mask_write_register(unit_id, address, and_mask, or_mask)) {
        Ok(()) => MbusGoStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}

/// Reads and simultaneously writes multiple holding registers (FC23).
///
/// # Safety
///
/// `handle`, `write_values`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_write_multiple_registers(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    read_address: u16,
    read_quantity: u16,
    write_address: u16,
    write_values: *const u16,
    write_quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if write_values.is_null() || out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    if out_buf_len < read_quantity {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let wv = unsafe { slice::from_raw_parts(write_values, write_quantity as usize) };
    let rt = runtime::get();
    let regs = match rt.block_on(client.read_write_multiple_registers(
        unit_id,
        read_address,
        read_quantity,
        write_address,
        wv,
    )) {
        Ok(r) => r,
        Err(e) => return status::from_async(e),
    };
    let qty = regs.quantity();
    let base = regs.from_address();
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, qty as usize) };
    for (i, slot) in dst.iter_mut().enumerate() {
        *slot = regs.value(base + i as u16).unwrap_or(0);
    }
    unsafe { *out_count = qty };
    MbusGoStatus::MbusOk
}

// ── Coil FCs ─────────────────────────────────────────────────────────────────

/// Reads `quantity` coils (FC01).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_coils(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let byte_count = quantity.div_ceil(8);
    if out_buf_len < byte_count {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let rt = runtime::get();
    let coils = match rt.block_on(client.read_multiple_coils(unit_id, address, quantity)) {
        Ok(c) => c,
        Err(e) => return status::from_async(e),
    };
    let src = coils.values();
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, byte_count as usize) };
    dst.copy_from_slice(&src[..byte_count as usize]);
    unsafe { *out_count = byte_count };
    MbusGoStatus::MbusOk
}

/// Writes a single coil (FC05).
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_write_single_coil(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    value: u8,
    out_address: *mut u16,
    out_value: *mut u8,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.write_single_coil(unit_id, address, value != 0)) {
        Ok((addr, on)) => {
            if !out_address.is_null() {
                unsafe { *out_address = addr };
            }
            if !out_value.is_null() {
                unsafe { *out_value = on as u8 };
            }
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Writes multiple coils (FC15).
///
/// # Safety
///
/// `handle` and `packed_coils` must be valid non-null pointers.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_write_multiple_coils(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    packed_coils: *const u8,
    byte_count: u16,
    coil_count: u16,
    out_address: *mut u16,
    out_quantity: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if packed_coils.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let packed = unsafe { slice::from_raw_parts(packed_coils, byte_count as usize) };
    let coils = match mbus_core::models::coil::Coils::new(address, coil_count) {
        Ok(c) => c,
        Err(e) => return status::from_mbus(e),
    };
    let coils = match coils.with_values(packed, coil_count) {
        Ok(c) => c,
        Err(e) => return status::from_mbus(e),
    };
    let rt = runtime::get();
    match rt.block_on(client.write_multiple_coils(unit_id, address, &coils)) {
        Ok((addr, qty)) => {
            if !out_address.is_null() {
                unsafe { *out_address = addr };
            }
            if !out_quantity.is_null() {
                unsafe { *out_quantity = qty };
            }
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── Discrete input FCs ────────────────────────────────────────────────────────

/// Reads `quantity` discrete inputs (FC02).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_discrete_inputs(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let byte_count = quantity.div_ceil(8);
    if out_buf_len < byte_count {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let rt = runtime::get();
    let di = match rt.block_on(client.read_discrete_inputs(unit_id, address, quantity)) {
        Ok(d) => d,
        Err(e) => return status::from_async(e),
    };
    let src = di.values();
    let n = src.len().min(byte_count as usize);
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, n) };
    dst.copy_from_slice(&src[..n]);
    unsafe { *out_count = n as u16 };
    MbusGoStatus::MbusOk
}

// ── FIFO FC ───────────────────────────────────────────────────────────────────

/// Reads the FIFO queue (FC24).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_fifo_queue(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    address: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    let fifo = match rt.block_on(client.read_fifo_queue(unit_id, address)) {
        Ok(f) => f,
        Err(e) => return status::from_async(e),
    };
    let values = fifo.queue();
    if out_buf_len < values.len() as u16 {
        return MbusGoStatus::MbusErrBufferTooSmall;
    }
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, values.len()) };
    dst.copy_from_slice(values);
    unsafe { *out_count = values.len() as u16 };
    MbusGoStatus::MbusOk
}

// ── Diagnostics FCs ───────────────────────────────────────────────────────────

/// Reads the device exception status (FC07).
///
/// # Safety
///
/// `handle` and `out_status` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_exception_status(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    out_status: *mut u8,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_status.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.read_exception_status(unit_id)) {
        Ok(s) => {
            unsafe { *out_status = s };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reads the communication event counter (FC11).
///
/// # Safety
///
/// `handle`, `out_status`, and `out_event_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_get_comm_event_counter(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    out_status: *mut u16,
    out_event_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_status.is_null() || out_event_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.get_comm_event_counter(unit_id)) {
        Ok((st, cnt)) => {
            unsafe { *out_status = st };
            unsafe { *out_event_count = cnt };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reads the communication event log (FC12).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_get_comm_event_log(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.get_comm_event_log(unit_id)) {
        Ok((_st, _ec, _mc, events)) => {
            if out_buf_len < events.len() as u16 {
                return MbusGoStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, events.len()) };
            dst.copy_from_slice(&events);
            unsafe { *out_count = events.len() as u16 };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reports the server identifier (FC17).
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_report_server_id(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.report_server_id(unit_id)) {
        Ok(data) => {
            if out_buf_len < data.len() as u16 {
                return MbusGoStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, data.len()) };
            dst.copy_from_slice(&data);
            unsafe { *out_count = data.len() as u16 };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── Diagnostics FC08 ─────────────────────────────────────────────────────────

/// Sends a Diagnostics (FC08) request.
///
/// # Safety
///
/// `handle`, `out_sub_function`, `out_buf`, and `out_count` must be valid non-null
/// pointers. If `data_in_count > 0`, `data_in` must point to at least `data_in_count`
/// valid `u16` words.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_diagnostics(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    sub_function: u16,
    data_in: *const u16,
    data_in_count: u16,
    out_sub_function: *mut u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if out_sub_function.is_null() || out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    if data_in_count > 0 && data_in.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let sf = match DiagnosticSubFunction::try_from(sub_function) {
        Ok(s) => s,
        Err(e) => return status::from_mbus(e),
    };
    let words: &[u16] = if data_in_count > 0 {
        unsafe { slice::from_raw_parts(data_in, data_in_count as usize) }
    } else {
        &[]
    };
    let rt = runtime::get();
    match rt.block_on(client.diagnostics(unit_id, sf, words)) {
        Ok(resp) => {
            if out_buf_len < resp.data.len() as u16 {
                return MbusGoStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, resp.data.len()) };
            dst.copy_from_slice(&resp.data);
            unsafe { *out_sub_function = u16::from(resp.sub_function) };
            unsafe { *out_count = resp.data.len() as u16 };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── File Record FCs ───────────────────────────────────────────────────────────

/// A sub-request descriptor for serial file record operations.
///
/// Identical layout to `MbusGoSubRequest` in `tcp.rs` — shared by both
/// transport variants so Go can use the same struct definition.
#[cfg(feature = "file-record")]
#[repr(C)]
pub struct MbusGoSerialSubRequest {
    /// File number (1–65535).
    pub file_number: u16,
    /// Starting record number within the file (0–9999).
    pub record_number: u16,
    /// Number of 16-bit registers. For reads: how many to read; for writes: must equal `data_len`.
    pub record_length: u16,
    /// Pointer to write data (`null` for reads).
    pub data: *const u16,
    /// Number of valid words pointed to by `data` (0 for reads).
    pub data_len: u16,
}

/// Reads one or more file records (FC14) over a serial connection.
///
/// # Safety
///
/// `handle`, `sub_reqs`, `out_buf`, and `out_count` must be valid non-null pointers.
/// `sub_reqs` must be valid for `sub_req_count` items.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_read_file_record(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    sub_reqs: *const MbusGoSerialSubRequest,
    sub_req_count: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if sub_reqs.is_null() || out_buf.is_null() || out_count.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let c_slice = unsafe { slice::from_raw_parts(sub_reqs, sub_req_count as usize) };
    let mut sub_request = SubRequest::new();
    for sr in c_slice {
        if let Err(e) =
            sub_request.add_read_sub_request(sr.file_number, sr.record_number, sr.record_length)
        {
            return status::from_mbus(e);
        }
    }
    let rt = runtime::get();
    match rt.block_on(client.read_file_record(unit_id, &sub_request)) {
        Ok(results) => {
            let total: usize = results
                .iter()
                .map(|r| r.record_data.as_ref().map_or(0, |d| d.len()))
                .sum();
            if out_buf_len < total as u16 {
                return MbusGoStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, total) };
            let mut pos = 0;
            for r in &results {
                if let Some(ref d) = r.record_data {
                    dst[pos..pos + d.len()].copy_from_slice(d.as_slice());
                    pos += d.len();
                }
            }
            unsafe { *out_count = total as u16 };
            MbusGoStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Writes one or more file records (FC15) over a serial connection.
///
/// # Safety
///
/// `handle` and `sub_reqs` must be valid non-null pointers. `sub_reqs` must be valid
/// for `sub_req_count` items. For each item, `data` must be valid for `data_len` words.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_serial_client_write_file_record(
    handle: *mut MbusGoSerialClient,
    unit_id: u8,
    sub_reqs: *const MbusGoSerialSubRequest,
    sub_req_count: u16,
) -> MbusGoStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    if sub_reqs.is_null() {
        return MbusGoStatus::MbusErrNullPointer;
    }
    let c_slice = unsafe { slice::from_raw_parts(sub_reqs, sub_req_count as usize) };
    let mut sub_request = SubRequest::new();
    for sr in c_slice {
        if sr.data.is_null() || sr.data_len == 0 {
            return MbusGoStatus::MbusErrNullPointer;
        }
        let word_slice = unsafe { slice::from_raw_parts(sr.data, sr.data_len as usize) };
        let mut hvec: HVec<u16, MAX_PDU_DATA_LEN> = HVec::new();
        if hvec.extend_from_slice(word_slice).is_err() {
            return MbusGoStatus::MbusErrBufferTooSmall;
        }
        if let Err(e) =
            sub_request.add_write_sub_request(sr.file_number, sr.record_number, sr.record_length, hvec)
        {
            return status::from_mbus(e);
        }
    }
    let rt = runtime::get();
    match rt.block_on(client.write_file_record(unit_id, &sub_request)) {
        Ok(()) => MbusGoStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}
