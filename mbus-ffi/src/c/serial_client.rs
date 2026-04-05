//! Modbus Serial (RTU / ASCII) client — ID-based C API.

use mbus_client::services::ClientServices;
use mbus_core::transport::SerialMode;

use super::app::CApp;
use super::callbacks::MbusCallbacks;
use super::config::{MbusSerialConfig, serial_config_from_c};
use super::error::MbusStatusCode;
use super::pool::{MbusClientId, MBUS_INVALID_CLIENT_ID, pool_allocate_serial, pool_free, pool_get_serial};
use super::transport::{CTransport, MbusTransportCallbacks, validate_transport_callbacks};

// ── Lifecycle ─────────────────────────────────────────────────────────────────

/// Create a new Modbus Serial client.
///
/// Returns a `MbusClientId` on success, or `MBUS_INVALID_CLIENT_ID` on failure.
///
/// # Safety
/// `config`, `transport_callbacks`, and `callbacks` must be valid non-null
/// pointers for the duration of this call. They are not retained after the
/// call returns.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_client_new(
    config: *const MbusSerialConfig,
    transport_callbacks: *const MbusTransportCallbacks,
    callbacks: *const MbusCallbacks,
) -> MbusClientId {
    if callbacks.is_null() || transport_callbacks.is_null() {
        return MBUS_INVALID_CLIENT_ID;
    }

    let serial_config = match unsafe { serial_config_from_c(config) } {
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
    let transport = CTransport::new_serial(
        transport_cb,
        match serial_config.mode {
            SerialMode::Rtu => SerialMode::Rtu,
            SerialMode::Ascii => SerialMode::Ascii,
        },
    );

    let inner = match ClientServices::new_serial(transport, app, serial_config) {
        Ok(i) => i,
        Err(_) => return MBUS_INVALID_CLIENT_ID,
    };

    match pool_allocate_serial(inner) {
        Ok(id) => id,
        Err(_) => MBUS_INVALID_CLIENT_ID,
    }
}

/// Free a Modbus Serial client created by [`mbus_serial_client_new`].
///
/// After this call the ID is invalid and must not be used.
/// Passing `MBUS_INVALID_CLIENT_ID` is a no-op.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_client_free(id: MbusClientId) {
    if id != MBUS_INVALID_CLIENT_ID {
        pool_free(id);
    }
}

// ── Connection management ─────────────────────────────────────────────────────

/// Open the serial port with the configured parameters.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_connect(id: MbusClientId) -> MbusStatusCode {
    let inner = match pool_get_serial(id) {
        Ok(c) => c,
        Err(e) => return e,
    };
    match inner.reconnect() {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}

/// Close the serial port (currently unsupported).
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_disconnect(id: MbusClientId) -> MbusStatusCode {
    match pool_get_serial(id) {
        Ok(_) => MbusStatusCode::MbusErrUnsupportedFunction,
        Err(e) => e,
    }
}

/// Returns `1` if the serial port is currently open, `0` otherwise.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_is_connected(id: MbusClientId) -> u8 {
    match pool_get_serial(id) {
        Ok(inner) => if inner.is_connected() { 1 } else { 0 },
        Err(_) => 0,
    }
}

// ── Poll ──────────────────────────────────────────────────────────────────────

/// Drive the serial Modbus state machine once.
///
/// Call periodically from your application loop. All registered callbacks are
/// invoked synchronously from within this call.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_poll(id: MbusClientId) {
    if let Ok(inner) = pool_get_serial(id) {
        inner.poll();
    }
}

/// Disconnect then reconnect the serial port.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_reconnect(id: MbusClientId) -> MbusStatusCode {
    let inner = match pool_get_serial(id) {
        Ok(c) => c,
        Err(e) => return e,
    };
    match inner.reconnect() {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}
