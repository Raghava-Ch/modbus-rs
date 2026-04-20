//! Modbus TCP client — ID-based C API.

use mbus_client::services::ClientServices;

use super::app::CApp;
use super::callbacks::MbusCallbacks;
use super::config::{MbusTcpConfig, tcp_config_from_c};
use super::error::MbusStatusCode;
use super::pool::{
    MBUS_INVALID_CLIENT_ID, MbusClientId, pool_allocate_tcp, pool_free, with_tcp_client,
};
use super::transport::{CTcpTransport, MbusTransportCallbacks, validate_transport_callbacks};

// ── Lifecycle ─────────────────────────────────────────────────────────────────

/// Create a new Modbus TCP client.
///
/// - `config`    — Must be a valid, non-null pointer to an initialised [`MbusTcpConfig`].
/// - `transport_callbacks` — Must define connect/disconnect/send/recv/is_connected.
/// - `callbacks` — Must be a valid, non-null pointer to an initialised [`MbusCallbacks`].
///
/// Returns a numeric `MbusClientId` on success, or `MBUS_INVALID_CLIENT_ID`
/// (0xFF) on failure (e.g. invalid config, pool full).
///
/// # Safety
/// `config`, `transport_callbacks`, and `callbacks` must be valid pointers for
/// the duration of this call. They are not retained after the call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_client_new(
    config: *const MbusTcpConfig,
    transport_callbacks: *const MbusTransportCallbacks,
    callbacks: *const MbusCallbacks,
) -> MbusClientId {
    if callbacks.is_null() || transport_callbacks.is_null() {
        return MBUS_INVALID_CLIENT_ID;
    }

    let modbus_config = match unsafe { tcp_config_from_c(config) } {
        Ok(c) => c,
        Err(_) => return MBUS_INVALID_CLIENT_ID,
    };

    let cb = unsafe { callbacks.read() };
    if cb.on_current_millis.is_none() {
        return MBUS_INVALID_CLIENT_ID;
    }
    let transport_cb = unsafe { transport_callbacks.read() };
    if !validate_transport_callbacks(&transport_cb) {
        return MBUS_INVALID_CLIENT_ID;
    }
    let app = CApp::new(cb);
    let transport = CTcpTransport::new(transport_cb);

    let inner = match ClientServices::new(transport, app, modbus_config) {
        Ok(i) => i,
        Err(_) => return MBUS_INVALID_CLIENT_ID,
    };

    match pool_allocate_tcp(inner) {
        Ok(id) => id,
        Err(_) => MBUS_INVALID_CLIENT_ID,
    }
}

/// Free a Modbus TCP client created by [`mbus_tcp_client_new`].
///
/// After this call the ID is invalid and must not be used.
/// Passing `MBUS_INVALID_CLIENT_ID` is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_client_free(id: MbusClientId) {
    if id != MBUS_INVALID_CLIENT_ID {
        pool_free(id);
    }
}

// ── Connection management ─────────────────────────────────────────────────────

/// Open the TCP connection to the configured host:port.
///
/// Returns `MBUS_OK` on success or a specific error code on failure.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_connect(id: MbusClientId) -> MbusStatusCode {
    with_tcp_client(id, |inner| match inner.reconnect() {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    })
    .unwrap_or_else(|e| e)
}

/// Close the TCP connection.
///
/// Pending in-flight requests are failed immediately with `MBUS_ERR_CONNECTION_LOST`.
/// The client ID remains valid; call [`mbus_tcp_connect`] to reconnect.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_disconnect(id: MbusClientId) -> MbusStatusCode {
    with_tcp_client(id, |inner| {
        inner.disconnect();
        MbusStatusCode::MbusOk
    })
    .unwrap_or_else(|e| e)
}

/// Returns `1` if the TCP connection is currently open, `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_is_connected(id: MbusClientId) -> u8 {
    with_tcp_client(id, |inner| if inner.is_connected() { 1 } else { 0 }).unwrap_or(0)
}

// ── Poll ──────────────────────────────────────────────────────────────────────

/// Drive the Modbus state machine: receive pending frames, match responses to
/// outstanding requests, fire any ready callbacks, and handle timeouts / retries.
///
/// Call this function periodically (e.g. every 5–20 ms) from your application
/// loop. All registered callbacks are invoked synchronously from within this call.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_poll(id: MbusClientId) {
    let _ = with_tcp_client(id, |inner| inner.poll());
}

/// Returns `1` if the client has in-flight requests awaiting response/timeout,
/// `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_has_pending_requests(id: MbusClientId) -> u8 {
    with_tcp_client(id, |inner| if inner.has_pending_requests() { 1 } else { 0 }).unwrap_or(0)
}

// ── Reconnect helper ──────────────────────────────────────────────────────────

/// Disconnect then reconnect. Useful after a `MBUS_ERR_CONNECTION_LOST` callback.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_reconnect(id: MbusClientId) -> MbusStatusCode {
    with_tcp_client(id, |inner| match inner.reconnect() {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    })
    .unwrap_or_else(|e| e)
}
