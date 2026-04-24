//! C API lifecycle functions for Modbus TCP servers.
//!
//! # Usage
//!
//! ```text
//! MbusServerId id = mbus_tcp_server_new(&transport_callbacks, &server_handlers, &config);
//! mbus_tcp_server_connect(id);
//! while (running) {
//!     mbus_server_lock(id);
//!     mbus_tcp_server_poll(id);
//!     mbus_server_unlock(id);
//! }
//! mbus_tcp_server_disconnect(id);
//! mbus_tcp_server_free(id);
//! ```
//!
//! # Thread Safety
//!
//! `mbus_tcp_server_poll`, `mbus_tcp_server_connect`, and `mbus_tcp_server_disconnect`
//! must be called with the server lock held (`mbus_server_lock` / `mbus_server_unlock`).
//! Pool-mutating calls (`mbus_tcp_server_new` / `mbus_tcp_server_free`) use the pool lock
//! internally; callers must hold `mbus_server_pool_lock` externally if they need atomicity
//! with ID inspection.

use mbus_server::ServerServices;

use crate::c::{
    error::MbusStatusCode,
    transport::{CTcpTransport, MbusTransportCallbacks, validate_transport_callbacks},
};

use super::{
    app::CServerApp,
    callbacks::MbusServerHandlers,
    config::MbusServerConfig,
    pool::{MBUS_INVALID_SERVER_ID, MbusServerId, server_pool_allocate_tcp, server_pool_free, with_tcp_server},
};

// ── mbus_tcp_server_new ───────────────────────────────────────────────────────

/// Creates a new Modbus TCP server and returns an opaque server ID.
///
/// # Parameters
/// - `transport` — Transport callbacks providing connect/disconnect/send/recv operations.
/// - `handlers`  — Application callback table. NULL callback slots respond with
///   `IllegalFunction`.
/// - `config`    — Server configuration (slave address, timeouts).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` if the pool
/// is full or the configuration is invalid.
///
/// # Safety
/// - `transport` and `handlers` must be non-null for the lifetime of the server.
/// - All function pointers in both structs must be valid.
/// - `handlers.userdata` must outlive the server.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_server_new(
    transport: *const MbusTransportCallbacks,
    handlers: *const MbusServerHandlers,
    config: *const MbusServerConfig,
) -> MbusServerId {
    if transport.is_null() || handlers.is_null() || config.is_null() {
        return MBUS_INVALID_SERVER_ID;
    }

    let transport_callbacks = unsafe { transport.read() };
    let server_handlers = unsafe { handlers.read() };
    let server_config = unsafe { *config };

    if !validate_transport_callbacks(&transport_callbacks) {
        return MBUS_INVALID_SERVER_ID;
    }

    let unit_id = match server_config.unit_id_or_slave_addr() {
        Ok(u) => u,
        Err(_) => return MBUS_INVALID_SERVER_ID,
    };

    let modbus_config = match server_config.tcp_modbus_config() {
        Ok(c) => c,
        Err(_) => return MBUS_INVALID_SERVER_ID,
    };

    let transport = CTcpTransport::new(transport_callbacks);
    let app = CServerApp::new(server_handlers);
    let resilience = server_config.resilience();

    let inner = ServerServices::new(transport, app, modbus_config, unit_id, resilience);

    match server_pool_allocate_tcp(inner) {
        Ok(id) => id,
        Err(_) => MBUS_INVALID_SERVER_ID,
    }
}

// ── mbus_tcp_server_free ──────────────────────────────────────────────────────

/// Destroys a TCP server and releases its pool slot.
///
/// The server's transport is NOT automatically disconnected before freeing. Call
/// `mbus_tcp_server_disconnect` first if the transport is still connected.
///
/// After this call, `id` is invalid and must not be used.
///
/// # Safety
/// The caller must hold the server lock (`mbus_server_lock(id)`) before calling this.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_free(id: MbusServerId) {
    server_pool_free(id);
}

// ── mbus_tcp_server_connect ───────────────────────────────────────────────────

/// Opens the server's transport (e.g. begins listening for connections).
///
/// For the C transport model, "connect" invokes the `connect` callback supplied
/// to `mbus_tcp_server_new`. For a Modbus TCP server this typically means
/// starting the listening socket and accepting a connection.
///
/// # Returns
/// `MbusOk` on success, or an error code if the transport callback failed.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_connect(id: MbusServerId) -> MbusStatusCode {
    match with_tcp_server(id, |srv| srv.connect().map_err(MbusStatusCode::from)) {
        Ok(Ok(())) => MbusStatusCode::MbusOk,
        Ok(Err(e)) => e,
        Err(e) => e,
    }
}

// ── mbus_tcp_server_disconnect ────────────────────────────────────────────────

/// Closes the server's transport.
///
/// Invokes the `disconnect` transport callback.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_disconnect(id: MbusServerId) -> MbusStatusCode {
    match with_tcp_server(id, |srv| {
        srv.disconnect();
    }) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => e,
    }
}

// ── mbus_tcp_server_poll ──────────────────────────────────────────────────────

/// Drives the server state machine for one iteration.
///
/// Must be called in a tight loop or event loop. Each call:
/// 1. Retries any queued, undelivered responses.
/// 2. Reads bytes from the transport.
/// 3. Parses complete frames and dispatches them to the application callbacks.
/// 4. Sends responses back over the transport.
///
/// The server lock must be held while calling this function.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_poll(id: MbusServerId) -> MbusStatusCode {
    match with_tcp_server(id, |srv| {
        srv.poll();
    }) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => e,
    }
}

// ── mbus_tcp_server_is_connected ──────────────────────────────────────────────

/// Returns `true` if the server's transport reports itself as connected.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_is_connected(id: MbusServerId) -> bool {
    with_tcp_server(id, |srv| srv.is_connected()).unwrap_or(false)
}

// ── mbus_tcp_server_pending_request_count ─────────────────────────────────────

/// Returns the number of requests currently waiting in the priority queue.
///
/// Non-zero when priority queuing is enabled in the resilience config.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_pending_request_count(id: MbusServerId) -> usize {
    with_tcp_server(id, |srv| srv.pending_request_count()).unwrap_or(0)
}

// ── mbus_tcp_server_pending_response_count ────────────────────────────────────

/// Returns the number of responses waiting for retry (failed sends).
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_pending_response_count(id: MbusServerId) -> usize {
    with_tcp_server(id, |srv| srv.pending_response_count()).unwrap_or(0)
}
