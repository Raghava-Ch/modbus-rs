//! .NET P/Invoke surface for [`mbus_client_async::AsyncTcpClient`].
//!
//! The C# wrapper holds the raw pointer in a `SafeHandle`; every entry
//! point is `extern "C"` and uses only POD parameter types so it can be
//! consumed by `[DllImport]` / `[LibraryImport]` declarations.
//!
//! ## Calling convention
//!
//! * Constructors return a non-null `*mut MbusDnTcpClient` on success or
//!   `null` on configuration failure.
//! * Every other function returns a [`MbusDnStatus`] code.  `MbusOk` (0)
//!   means success; non-zero values map 1:1 onto the C-binding status enum.
//! * Out-parameters (`out_count`, register/coil buffers) are written only
//!   when the function returns `MbusOk`.  On error the buffer contents are
//!   unspecified.
//!
//! Every request entry point blocks the calling thread on the shared
//! [`crate::dotnet::runtime`] until the underlying async operation completes.
//! The C# wrapper hides this inside `Task.Run` so callers `await` a
//! `Task<T>` as usual.

use core::ffi::{c_char, c_void};
use core::ptr;
use core::slice;
use std::ffi::CStr;
use std::sync::Arc;
use std::time::Duration;

use mbus_client_async::AsyncTcpClient;

use crate::dotnet::runtime;
use crate::dotnet::status::{self, MbusDnStatus};

/// Opaque handle to an asynchronous Modbus TCP client.
///
/// Created by [`mbus_dn_tcp_client_new`] and destroyed by
/// [`mbus_dn_tcp_client_free`].  Always passed by raw pointer over FFI.
///
/// The struct itself is heap-allocated (`Box::into_raw`); `Arc` lets the
/// shared Tokio runtime hold a clone for the lifetime of any in-flight
/// request without preventing destruction once the C# `SafeHandle` runs
/// `_free`.
#[allow(missing_docs)]
pub struct MbusDnTcpClient {
    inner: Arc<AsyncTcpClient>,
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Creates a new async TCP client targeting `host:port`.
///
/// `host` must be a NUL-terminated UTF-8 string.  The returned pointer
/// must eventually be released with [`mbus_dn_tcp_client_free`].  Returns
/// `null` if `host` is null, not valid UTF-8, or the underlying constructor
/// fails (for example because no Tokio runtime could be started).
///
/// # Safety
///
/// `host` must point to a valid NUL-terminated string for the duration of
/// this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_new(
    host: *const c_char,
    port: u16,
) -> *mut MbusDnTcpClient {
    if host.is_null() {
        return ptr::null_mut();
    }
    let host_str = match unsafe { CStr::from_ptr(host) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    // AsyncTcpClient::new() requires an active tokio runtime context to spawn
    // its background task.  Enter the shared runtime for the duration of the
    // call.
    let rt = runtime::get();
    let _guard = rt.enter();
    let client = match AsyncTcpClient::new(host_str, port) {
        Ok(c) => c,
        Err(_) => return ptr::null_mut(),
    };

    Box::into_raw(Box::new(MbusDnTcpClient {
        inner: Arc::new(client),
    }))
}

/// Releases an `MbusDnTcpClient` previously returned from
/// [`mbus_dn_tcp_client_new`].
///
/// Drops the underlying `AsyncTcpClient`, which signals its background
/// Tokio task to exit.  No-op if `handle` is null.  Safe to call exactly
/// once per handle.
///
/// # Safety
///
/// `handle` must be either null or a pointer previously returned from
/// [`mbus_dn_tcp_client_new`] that has not already been freed.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_free(handle: *mut MbusDnTcpClient) {
    if handle.is_null() {
        return;
    }
    drop(unsafe { Box::from_raw(handle) });
}

// ── Connection management ────────────────────────────────────────────────────

/// Establishes the TCP transport connection.
///
/// Blocks the calling thread until the connection completes or fails.
///
/// # Safety
///
/// `handle` must be a non-null pointer previously returned from
/// [`mbus_dn_tcp_client_new`] and still alive.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_connect(handle: *mut MbusDnTcpClient) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.connect()) {
        Ok(()) => MbusDnStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}

/// Closes the TCP transport gracefully.
///
/// Drains any in-flight or queued requests with `ConnectionClosed`.  After
/// this call the client can be reconnected with
/// [`mbus_dn_tcp_client_connect`].
///
/// # Safety
///
/// See [`mbus_dn_tcp_client_connect`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_disconnect(
    handle: *mut MbusDnTcpClient,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.disconnect()) {
        Ok(()) => MbusDnStatus::MbusOk,
        Err(e) => status::from_async(e),
    }
}

/// Sets a per-request timeout in milliseconds; `0` disables the timeout.
///
/// # Safety
///
/// See [`mbus_dn_tcp_client_connect`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_set_request_timeout_ms(
    handle: *mut MbusDnTcpClient,
    timeout_ms: u64,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if timeout_ms == 0 {
        client.clear_request_timeout();
    } else {
        client.set_request_timeout(Duration::from_millis(timeout_ms));
    }
    MbusDnStatus::MbusOk
}

/// Returns `1` when there are requests in flight awaiting a response, `0`
/// otherwise; returns `0` when `handle` is null.
///
/// # Safety
///
/// See [`mbus_dn_tcp_client_connect`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_has_pending_requests(
    handle: *mut MbusDnTcpClient,
) -> u8 {
    match unsafe { handle.as_ref() } {
        Some(h) => h.inner.has_pending_requests() as u8,
        None => 0,
    }
}

// ── Request entry points ─────────────────────────────────────────────────────

/// Reads `quantity` holding registers (FC03) starting at `address` from
/// the given `unit_id` and copies them, in declaration order, into the
/// caller-supplied `out_buf` of length `out_buf_len` (in `u16` elements).
///
/// On success writes the number of registers actually read into
/// `out_count` (which equals `quantity`) and returns `MbusOk`.
///
/// Returns `MbusErrBufferTooSmall` if `out_buf_len < quantity`.
///
/// # Safety
///
/// * `handle` must be a valid client pointer.
/// * `out_buf` must point to writable storage for at least `out_buf_len`
///   `u16` values.
/// * `out_count` must point to a writable `u16`.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_holding_registers(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    if out_buf_len < quantity {
        return MbusDnStatus::MbusErrBufferTooSmall;
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
    MbusDnStatus::MbusOk
}

/// Writes a single holding register (FC06).
///
/// On success, writes the echoed `(address, value)` to `out_address` and
/// `out_value` if those pointers are non-null.
///
/// # Safety
///
/// * `handle` must be a valid client pointer.
/// * `out_address` and `out_value` may be null; if non-null they must
///   point to writable `u16` storage.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_write_single_register(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    value: u16,
    out_address: *mut u16,
    out_value: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
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
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Writes `quantity` holding registers (FC16) starting at `address`.
///
/// `values` must point to `quantity` `u16` values in declaration order.
///
/// On success, writes the echoed `(starting_address, quantity)` to
/// `out_address` and `out_quantity` if non-null.
///
/// # Safety
///
/// * `handle` must be a valid client pointer.
/// * `values` must point to at least `quantity` readable `u16` values.
/// * `out_address` and `out_quantity` may be null.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_write_multiple_registers(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    values: *const u16,
    quantity: u16,
    out_address: *mut u16,
    out_quantity: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if values.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    if quantity == 0 {
        return MbusDnStatus::MbusErrInvalidQuantity;
    }
    let slice = unsafe { slice::from_raw_parts(values, quantity as usize) };

    let rt = runtime::get();
    match rt.block_on(client.write_multiple_registers(unit_id, address, slice)) {
        Ok((addr, qty)) => {
            if !out_address.is_null() {
                unsafe { *out_address = addr };
            }
            if !out_quantity.is_null() {
                unsafe { *out_quantity = qty };
            }
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── Coil FCs ─────────────────────────────────────────────────────────────────

/// Reads `quantity` coils (FC01) starting at `address` from `unit_id`.
///
/// Writes bit-packed coil bytes (Modbus wire format, LSB-first) into `out_buf`.
/// `out_buf_len` is the buffer length in bytes; must be ≥ `ceil(quantity / 8)`.
/// On success writes the byte count to `out_count`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_coils(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let byte_count = quantity.div_ceil(8);
    if out_buf_len < byte_count {
        return MbusDnStatus::MbusErrBufferTooSmall;
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
    MbusDnStatus::MbusOk
}

/// Writes a single coil (FC05). `value != 0` means ON.
///
/// On success, writes echoed address to `out_address` and value (`1`=ON, `0`=OFF)
/// to `out_value` if those pointers are non-null.
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_write_single_coil(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    value: u8,
    out_address: *mut u16,
    out_value: *mut u8,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
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
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Writes multiple coils (FC15) starting at `address`.
///
/// `packed_coils` is Modbus bit-packed coil data (LSB-first). `byte_count` is
/// the number of bytes in `packed_coils`; `coil_count` is the number of coils.
///
/// # Safety
///
/// `handle` and `packed_coils` must be valid non-null pointers.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_write_multiple_coils(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    packed_coils: *const u8,
    byte_count: u16,
    coil_count: u16,
    out_address: *mut u16,
    out_quantity: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if packed_coils.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
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
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── Discrete input FCs ────────────────────────────────────────────────────────

/// Reads `quantity` discrete inputs (FC02) starting at `address` from `unit_id`.
///
/// Writes bit-packed bytes into `out_buf`. On success writes byte count to `out_count`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_discrete_inputs(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let byte_count = quantity.div_ceil(8);
    if out_buf_len < byte_count {
        return MbusDnStatus::MbusErrBufferTooSmall;
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
    MbusDnStatus::MbusOk
}

// ── Input register FCs ────────────────────────────────────────────────────────

/// Reads `quantity` input registers (FC04) starting at `address` from `unit_id`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_input_registers(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    if out_buf_len < quantity {
        return MbusDnStatus::MbusErrBufferTooSmall;
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
    MbusDnStatus::MbusOk
}

/// Applies an AND/OR bitmask to a holding register (FC22).
///
/// # Safety
///
/// `handle` must be a valid client pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_mask_write_register(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    let rt = runtime::get();
    match rt.block_on(client.mask_write_register(unit_id, address, and_mask, or_mask)) {
        Ok(()) => MbusDnStatus::MbusOk,
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
pub unsafe extern "C" fn mbus_dn_tcp_client_read_write_multiple_registers(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    read_address: u16,
    read_quantity: u16,
    write_address: u16,
    write_values: *const u16,
    write_quantity: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if write_values.is_null() || out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    if out_buf_len < read_quantity {
        return MbusDnStatus::MbusErrBufferTooSmall;
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
    MbusDnStatus::MbusOk
}

// ── FIFO FC ───────────────────────────────────────────────────────────────────

/// Reads the FIFO queue (FC24) at `address` from `unit_id`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_fifo_queue(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    address: u16,
    out_buf: *mut u16,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    let fifo = match rt.block_on(client.read_fifo_queue(unit_id, address)) {
        Ok(f) => f,
        Err(e) => return status::from_async(e),
    };
    let values = fifo.queue();
    if out_buf_len < values.len() as u16 {
        return MbusDnStatus::MbusErrBufferTooSmall;
    }
    let dst = unsafe { slice::from_raw_parts_mut(out_buf, values.len()) };
    dst.copy_from_slice(values);
    unsafe { *out_count = values.len() as u16 };
    MbusDnStatus::MbusOk
}

// ── Diagnostics FCs ───────────────────────────────────────────────────────────

/// Reads the device exception status (FC07).
///
/// # Safety
///
/// `handle` and `out_status` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_read_exception_status(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    out_status: *mut u8,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_status.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.read_exception_status(unit_id)) {
        Ok(s) => {
            unsafe { *out_status = s };
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reads the communication event counter (FC11).
///
/// On success writes `status_word` and `event_count` to the respective out pointers.
///
/// # Safety
///
/// `handle`, `out_status`, and `out_event_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_get_comm_event_counter(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    out_status: *mut u16,
    out_event_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_status.is_null() || out_event_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.get_comm_event_counter(unit_id)) {
        Ok((st, cnt)) => {
            unsafe { *out_status = st };
            unsafe { *out_event_count = cnt };
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reads the communication event log (FC12).
///
/// Writes the raw event bytes into `out_buf`. On success writes byte count to `out_count`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_get_comm_event_log(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.get_comm_event_log(unit_id)) {
        Ok((_st, _ec, _mc, events)) => {
            if out_buf_len < events.len() as u16 {
                return MbusDnStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, events.len()) };
            dst.copy_from_slice(&events);
            unsafe { *out_count = events.len() as u16 };
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

/// Reports the server identifier (FC17).
///
/// Writes the raw server ID bytes into `out_buf`. On success writes byte count to `out_count`.
///
/// # Safety
///
/// `handle`, `out_buf`, and `out_count` must be valid non-null pointers.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_client_report_server_id(
    handle: *mut MbusDnTcpClient,
    unit_id: u8,
    out_buf: *mut u8,
    out_buf_len: u16,
    out_count: *mut u16,
) -> MbusDnStatus {
    let client = match unsafe { handle.as_ref() } {
        Some(h) => h.inner.clone(),
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if out_buf.is_null() || out_count.is_null() {
        return MbusDnStatus::MbusErrNullPointer;
    }
    let rt = runtime::get();
    match rt.block_on(client.report_server_id(unit_id)) {
        Ok(data) => {
            if out_buf_len < data.len() as u16 {
                return MbusDnStatus::MbusErrBufferTooSmall;
            }
            let dst = unsafe { slice::from_raw_parts_mut(out_buf, data.len()) };
            dst.copy_from_slice(&data);
            unsafe { *out_count = data.len() as u16 };
            MbusDnStatus::MbusOk
        }
        Err(e) => status::from_async(e),
    }
}

// ── cbindgen visibility helpers ──────────────────────────────────────────────
//
// `MbusDnTcpClient` is opaque from C#'s point of view; ensure cbindgen
// emits a forward declaration by referencing it from a `*mut c_void`
// helper.  The function itself does nothing useful at runtime.
#[doc(hidden)]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_dn_tcp_client_handle_size() -> usize {
    core::mem::size_of::<MbusDnTcpClient>()
}

#[doc(hidden)]
fn _opaque_marker(_p: *mut c_void) {}
