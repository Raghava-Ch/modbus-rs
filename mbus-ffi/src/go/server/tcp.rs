//! Go cgo surface for the async Modbus TCP server.
//!
//! # Lifecycle
//!
//! 1. Allocate a [`MbusGoServerVtable`] on the Go side and fill in function
//!    pointer slots for the function codes your application supports.
//! 2. Call [`mbus_go_tcp_server_new`] — it returns an opaque
//!    `*mut MbusGoTcpServer` handle (or `null` on failure).
//! 3. Call [`mbus_go_tcp_server_start`] from any thread — it spawns a
//!    background OS thread that blocks on `serve_with_shutdown`.
//! 4. Call [`mbus_go_tcp_server_stop`] to signal shutdown; the background
//!    thread exits shortly after.
//! 5. Call [`mbus_go_tcp_server_free`] to reclaim the handle memory.

use core::ffi::c_char;
use core::ptr;
use std::ffi::CStr;
use std::sync::Arc;

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server_async::AsyncTcpServer as InnerAsyncTcpServer;
use tokio::sync::Notify;

use crate::go::runtime;
use crate::go::status::{self, MbusGoStatus};
use super::vtable::{GoServerAdapter, MbusGoServerVtable};

/// Opaque handle to an async Modbus TCP server.
pub struct MbusGoTcpServer {
    bind_addr: String,
    unit_id: u8,
    vtable: Arc<MbusGoServerVtable>,
    stop_signal: Arc<Notify>,
}

// ── Lifecycle ────────────────────────────────────────────────────────────────

/// Creates a new TCP server handle.
///
/// `host` is a NUL-terminated bind address (e.g. `"0.0.0.0"`).
/// `unit_id` is the Modbus unit ID to respond to.
/// `vtable` must remain valid for the lifetime of the server handle.
///
/// Returns `null` if `host` or `vtable` is null, or if `unit_id` is invalid.
///
/// # Safety
///
/// * `host` must point to a valid NUL-terminated UTF-8 string.
/// * `vtable` must be non-null and the struct it points to must remain valid
///   until [`mbus_go_tcp_server_free`] is called.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_tcp_server_new(
    host: *const c_char,
    port: u16,
    unit_id: u8,
    vtable: *const MbusGoServerVtable,
) -> *mut MbusGoTcpServer {
    if host.is_null() || vtable.is_null() {
        return ptr::null_mut();
    }
    let host_str = match unsafe { CStr::from_ptr(host) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };
    if UnitIdOrSlaveAddr::new(unit_id).is_err() {
        return ptr::null_mut();
    }
    // Safety: the caller guarantees vtable outlives the handle.
    let vt: MbusGoServerVtable = unsafe { core::ptr::read(vtable) };
    Box::into_raw(Box::new(MbusGoTcpServer {
        bind_addr: format!("{host_str}:{port}"),
        unit_id,
        vtable: Arc::new(vt),
        stop_signal: Arc::new(Notify::new()),
    }))
}

/// Destroys the server handle.
///
/// Does **not** stop a running server — call [`mbus_go_tcp_server_stop`]
/// first, wait for the background thread to exit, then call this.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by [`mbus_go_tcp_server_new`].
/// Calling this more than once is undefined behaviour.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_tcp_server_free(handle: *mut MbusGoTcpServer) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

/// Starts the server on a new background OS thread.
///
/// The thread blocks on `serve_with_shutdown` until [`mbus_go_tcp_server_stop`]
/// is called or a fatal error occurs.  Returns immediately to the caller.
///
/// # Safety
///
/// `handle` must be a valid server pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_tcp_server_start(
    handle: *mut MbusGoTcpServer,
) -> MbusGoStatus {
    let srv = match unsafe { handle.as_ref() } {
        Some(s) => s,
        None => return MbusGoStatus::MbusErrNullPointer,
    };
    let unit = match UnitIdOrSlaveAddr::new(srv.unit_id) {
        Ok(u) => u,
        Err(e) => return status::from_mbus(e),
    };
    let addr = srv.bind_addr.clone();
    let adapter = GoServerAdapter::new_with_arc(srv.vtable.clone());
    let stop_signal = srv.stop_signal.clone();

    std::thread::spawn(move || {
        let rt = runtime::get();
        let _ = rt.block_on(InnerAsyncTcpServer::serve_with_shutdown(
            addr.as_str(),
            adapter,
            unit,
            stop_signal.notified(),
        ));
    });

    MbusGoStatus::MbusOk
}

/// Signals the running server to stop.
///
/// The background thread will finish in-flight sessions and then exit.
///
/// # Safety
///
/// `handle` must be a valid server pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_go_tcp_server_stop(handle: *mut MbusGoTcpServer) {
    if let Some(s) = unsafe { handle.as_ref() } {
        s.stop_signal.notify_one();
    }
}
