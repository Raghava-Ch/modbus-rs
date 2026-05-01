//! .NET P/Invoke surface for the async Modbus TCP gateway.
//!
//! # Lifecycle
//!
//! 1. Call [`mbus_dn_tcp_gateway_new`] with the upstream bind address.
//! 2. Register downstream channels with [`mbus_dn_tcp_gateway_add_downstream`].
//! 3. Add routing rules with [`mbus_dn_tcp_gateway_add_unit_route`] and/or
//!    [`mbus_dn_tcp_gateway_add_range_route`].
//! 4. Call [`mbus_dn_tcp_gateway_start`] to begin serving on a background thread.
//! 5. Call [`mbus_dn_tcp_gateway_stop`] to shut down.
//! 6. Call [`mbus_dn_tcp_gateway_free`] to release the handle.

use core::ffi::c_char;
use core::ptr;
use std::ffi::CStr;
use std::sync::{Arc, Mutex as StdMutex};

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::AsyncTcpGatewayServer;
use mbus_network::TokioTcpTransport;
use tokio::sync::{Mutex as TokioMutex, Notify};

use crate::dotnet::runtime;
use crate::dotnet::status::MbusDnStatus;

use super::router::DnRouter;

/// Opaque gateway handle.
pub struct MbusDnTcpGateway {
    inner: Arc<StdMutex<GatewayConfig>>,
    stop_signal: Arc<Notify>,
}

struct GatewayConfig {
    bind_addr: String,
    downstreams: Vec<(String, u16)>,
    router: DnRouter,
}

// ── Lifecycle ─────────────────────────────────────────────────────────────────

/// Creates a new gateway handle.
///
/// `host` is the NUL-terminated upstream bind address (e.g. `"0.0.0.0"`).
/// Returns `null` if `host` is null or not valid UTF-8.
///
/// # Safety
///
/// `host` must point to a valid NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_new(
    host: *const c_char,
    port: u16,
) -> *mut MbusDnTcpGateway {
    if host.is_null() {
        return ptr::null_mut();
    }
    let host_str = match unsafe { CStr::from_ptr(host) }.to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };
    let config = GatewayConfig {
        bind_addr: format!("{host_str}:{port}"),
        downstreams: Vec::new(),
        router: DnRouter::new(),
    };
    Box::into_raw(Box::new(MbusDnTcpGateway {
        inner: Arc::new(StdMutex::new(config)),
        stop_signal: Arc::new(Notify::new()),
    }))
}

/// Destroys the gateway handle.
///
/// Call [`mbus_dn_tcp_gateway_stop`] first if the gateway is running.
///
/// # Safety
///
/// `handle` must be a valid pointer returned by [`mbus_dn_tcp_gateway_new`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_free(handle: *mut MbusDnTcpGateway) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle) });
    }
}

// ── Configuration ─────────────────────────────────────────────────────────────

/// Adds a TCP downstream and returns its zero-based channel index.
///
/// Returns `u32::MAX` if `host` is null.
///
/// # Safety
///
/// `handle` and `host` must be valid non-null pointers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_add_downstream(
    handle: *mut MbusDnTcpGateway,
    host: *const c_char,
    port: u16,
) -> u32 {
    let gw = match unsafe { handle.as_ref() } {
        Some(g) => g,
        None => return u32::MAX,
    };
    if host.is_null() {
        return u32::MAX;
    }
    let host_str = match unsafe { CStr::from_ptr(host) }.to_str() {
        Ok(s) => s,
        Err(_) => return u32::MAX,
    };
    let mut cfg = gw.inner.lock().unwrap();
    cfg.downstreams.push((host_str.to_owned(), port));
    (cfg.downstreams.len() - 1) as u32
}

/// Routes a single unit ID to a downstream channel.
///
/// Returns `MbusOk` on success, `MbusErrInvalidAddress` if `unit_id == 0` or
/// `channel >= registered_downstream_count`.
///
/// # Safety
///
/// `handle` must be a valid gateway pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_add_unit_route(
    handle: *mut MbusDnTcpGateway,
    unit_id: u8,
    channel: u32,
) -> MbusDnStatus {
    let gw = match unsafe { handle.as_ref() } {
        Some(g) => g,
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if UnitIdOrSlaveAddr::new(unit_id).is_err() {
        return MbusDnStatus::MbusErrInvalidAddress;
    }
    let mut cfg = gw.inner.lock().unwrap();
    let ch = channel as usize;
    if ch >= cfg.downstreams.len() {
        return MbusDnStatus::MbusErrInvalidAddress;
    }
    cfg.router.add_unit(unit_id, ch);
    MbusDnStatus::MbusOk
}

/// Routes an inclusive unit-ID range `[min, max]` to a downstream channel.
///
/// Returns `MbusErrInvalidAddress` if `min == 0`, `min > max`, or
/// `channel >= registered_downstream_count`.
///
/// # Safety
///
/// `handle` must be a valid gateway pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_add_range_route(
    handle: *mut MbusDnTcpGateway,
    unit_min: u8,
    unit_max: u8,
    channel: u32,
) -> MbusDnStatus {
    let gw = match unsafe { handle.as_ref() } {
        Some(g) => g,
        None => return MbusDnStatus::MbusErrNullPointer,
    };
    if unit_min == 0 || unit_max < unit_min {
        return MbusDnStatus::MbusErrInvalidAddress;
    }
    let mut cfg = gw.inner.lock().unwrap();
    let ch = channel as usize;
    if ch >= cfg.downstreams.len() {
        return MbusDnStatus::MbusErrInvalidAddress;
    }
    cfg.router.add_range(unit_min, unit_max, ch);
    MbusDnStatus::MbusOk
}

// ── Serve / stop ──────────────────────────────────────────────────────────────

/// Starts the gateway on a new background OS thread.
///
/// Connects all registered downstreams and begins forwarding requests.
/// Returns `MbusOk` immediately; the thread runs until
/// [`mbus_dn_tcp_gateway_stop`] is called.
///
/// Returns `MbusErrInvalidAddress` if no downstreams or no routes have been
/// registered.
///
/// # Safety
///
/// `handle` must be a valid gateway pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_start(
    handle: *mut MbusDnTcpGateway,
) -> MbusDnStatus {
    let gw = match unsafe { handle.as_ref() } {
        Some(g) => g,
        None => return MbusDnStatus::MbusErrNullPointer,
    };

    let (bind_addr, downstreams, router) = {
        let cfg = gw.inner.lock().unwrap();
        if cfg.downstreams.is_empty() || cfg.router.is_empty() {
            return MbusDnStatus::MbusErrInvalidAddress;
        }
        (cfg.bind_addr.clone(), cfg.downstreams.clone(), cfg.router.clone())
    };
    let stop_signal = gw.stop_signal.clone();

    std::thread::spawn(move || {
        let rt = runtime::get();
        rt.block_on(async move {
            let mut ds = Vec::with_capacity(downstreams.len());
            for (host, port) in &downstreams {
                match TokioTcpTransport::connect((host.as_str(), *port)).await {
                    Ok(t) => ds.push(Arc::new(TokioMutex::new(t))),
                    Err(_) => return,
                }
            }
            let _ = AsyncTcpGatewayServer::serve_with_shutdown(
                bind_addr.as_str(),
                router,
                ds,
                stop_signal.notified(),
            )
            .await;
        });
    });

    MbusDnStatus::MbusOk
}

/// Signals the running gateway to stop.
///
/// # Safety
///
/// `handle` must be a valid gateway pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_dn_tcp_gateway_stop(handle: *mut MbusDnTcpGateway) {
    if let Some(gw) = unsafe { handle.as_ref() } {
        gw.stop_signal.notify_one();
    }
}
