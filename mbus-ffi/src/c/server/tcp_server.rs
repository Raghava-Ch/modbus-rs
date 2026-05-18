//! C API lifecycle functions for Modbus TCP servers.

#![cfg(all(feature = "c-server", feature = "network-tcp"))]
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
//!
//! # Error Returns
//!
//! All lifecycle functions that return `MbusStatusCode` will produce one of:
//!
//! | Code | Meaning |
//! |------|---------|
//! | `MbusOk` | Operation succeeded. |
//! | `MbusErrConnectionFailed` | The `connect` transport callback returned an error. |
//! | `MbusErrConnectionClosed` | The `disconnect` callback (or recv path) reported the connection as closed. |
//! | `MbusErrInvalidClientId` | The supplied `MbusServerId` is not in the TCP server pool (`MBUS_INVALID_SERVER_ID` or freed slot). |
//! | `MbusErrBusy` | (Reserved) Re-entrant call attempted while the lock is held by the same thread. |

use mbus_server::ServerServices;

use crate::c::{
    error::MbusStatusCode,
    transport::{CTcpTransport, MbusTransportCallbacks, validate_transport_callbacks},
};

use super::{
    app::CServerApp,
    callbacks::MbusServerHandlers,
    config::MbusServerConfig,
    pool::{
        MBUS_INVALID_SERVER_ID, MbusServerId, server_pool_allocate_tcp, server_pool_free,
        with_tcp_server,
    },
};

// ── mbus_tcp_server_new ───────────────────────────────────────────────────────

/// Creates a new Modbus TCP server and returns an opaque server ID.
///
/// # Parameters
/// - `transport` — Transport callbacks providing connect/disconnect/send/recv operations.
/// - `handlers`  — Application callback table.  Slots left `NULL` respond with
///   `IllegalFunction` automatically.
/// - `config`    — Server configuration (slave address, timeouts).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` on failure.
///
/// `MBUS_INVALID_SERVER_ID` is returned when:
/// - Any pointer argument is `NULL`.
/// - Any required function pointer inside `transport` is `NULL`.
/// - `config.slave_address` is out of the valid Modbus range (1–247).
/// - The internal server pool is exhausted (all slots occupied).
///
/// # Safety
/// - `transport`, `handlers`, and `config` must be non-null and remain valid for
///   the entire lifetime of the server (until `mbus_tcp_server_free` is called).
/// - All function pointers inside `transport` must be valid C functions.
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
/// The server's transport is **not** automatically disconnected before freeing.
/// Call `mbus_tcp_server_disconnect` first if the transport may still be active.
///
/// After this call, `id` becomes invalid and must not be reused.
///
/// # Error conditions (no return value)
/// This function is infallible — an invalid or already-freed `id` is silently
/// ignored to simplify cleanup paths.
///
/// # Safety
/// The caller is responsible for ensuring no other thread is using the server
/// (poll / connect / disconnect) concurrently when this is called.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_free(id: MbusServerId) {
    server_pool_free(id);
}

// ── mbus_tcp_server_connect ───────────────────────────────────────────────────

/// Opens the server's transport (e.g. begins accepting TCP connections).
///
/// Invokes the `connect` function-pointer from the `MbusTransportCallbacks` struct
/// supplied to `mbus_tcp_server_new`.
///
/// # Returns
/// - `MbusOk` — Transport connected successfully.
/// - `MbusErrConnectionFailed` — The `connect` callback returned a failure.
/// - `MbusErrIoError` — A lower-level I/O error occurred in the callback.
/// - `MbusErrInvalidConfiguration` — The transport was already connected, or the
///   configuration passed to the callback was rejected.
/// - `MbusErrInvalidClientId` — `id` does not refer to a live TCP server slot.
///
/// The server lock must be held while calling this function.
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
/// Invokes the `disconnect` function-pointer from `MbusTransportCallbacks`.
/// Outstanding queued responses are discarded; the server pool slot remains
/// valid and can be reconnected with `mbus_tcp_server_connect`.
///
/// # Returns
/// - `MbusOk` — Transport disconnected (or was already disconnected).
/// - `MbusErrInvalidClientId` — `id` does not refer to a live TCP server slot.
///
/// The server lock must be held while calling this function.
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
/// Must be called in a tight loop or cooperative event loop. Each call performs:
/// 1. **Response retry** — resends queued responses from previous failed sends.
/// 2. **Receive** — reads bytes from the transport `recv` callback.
/// 3. **Frame parse** — assembles complete Modbus MBAP + PDU frames.
/// 4. **Dispatch** — calls the matching `MbusServerHandlers` callback.
/// 5. **Send** — transmits the response over the transport `send` callback.
///
/// # Returns
/// `mbus_tcp_server_poll` itself always returns `MbusOk`; individual transport
/// failures are handled internally by the server's resilience layer:
///
/// | Internal event | Server behaviour |
/// |----------------|------------------|
/// | `recv` returns `Timeout` | No data this poll; move on silently. |
/// | `recv` returns `IoError` / `ConnectionClosed` | Transport is disconnected; server waits for reconnect. |
/// | `send` fails | Response is queued for retry on the next poll (up to `max_send_retries`). |
/// | Frame parse error | One byte is discarded and the sliding window re-syncs. |
///
/// - `MbusErrInvalidClientId` — `id` does not refer to a live TCP server slot.
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
///
/// Delegates to the `is_connected` function-pointer in `MbusTransportCallbacks`.
/// Returns `false` for an invalid or freed server ID.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_is_connected(id: MbusServerId) -> bool {
    with_tcp_server(id, |srv| srv.is_connected()).unwrap_or(false)
}

// ── mbus_tcp_server_pending_request_count ─────────────────────────────────────

/// Returns the number of requests currently waiting in the priority queue.
///
/// Reflects how many parsed requests are buffered waiting to be dispatched
/// (only non-zero when `ResilienceConfig::enable_priority_queue` is active).
/// Returns `0` for an invalid or freed server ID.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_pending_request_count(id: MbusServerId) -> usize {
    with_tcp_server(id, |srv| srv.pending_request_count()).unwrap_or(0)
}

// ── mbus_tcp_server_pending_response_count ────────────────────────────────────

/// Returns the number of responses waiting for retry (failed sends).
///
/// A non-zero value means the `send` transport callback has failed at least
/// once and the server is holding the unsent frames for retry.
/// Returns `0` for an invalid or freed server ID.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_server_pending_response_count(id: MbusServerId) -> usize {
    with_tcp_server(id, |srv| srv.pending_response_count()).unwrap_or(0)
}
