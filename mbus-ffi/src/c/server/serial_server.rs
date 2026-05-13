//! C API lifecycle functions for Modbus Serial servers (RTU and ASCII).

#![cfg(all(
    feature = "c-server",
    any(feature = "serial-rtu", feature = "serial-ascii")
))]
//!
//! Uses the same pool as TCP servers but a separate sub-pool with `QUEUE_DEPTH = 1`
//! (half-duplex serial allows only one request in flight at a time).
//!
//! # Usage
//!
//! ```text
//! MbusServerId id = mbus_serial_rtu_server_new(&transport_callbacks, &server_handlers, &config);
//! mbus_serial_server_connect(id);
//! while (running) {
//!     mbus_server_lock(id);
//!     mbus_serial_server_poll(id);
//!     mbus_server_unlock(id);
//! }
//! mbus_serial_server_disconnect(id);
//! mbus_serial_server_free(id);
//! ```

use mbus_server::ServerServices;

#[cfg(feature = "serial-ascii")]
use crate::c::transport::CAsciiTransport;
#[cfg(feature = "serial-rtu")]
use crate::c::transport::CRtuTransport;
use crate::c::{
    error::MbusStatusCode,
    transport::{MbusTransportCallbacks, validate_transport_callbacks},
};

#[cfg(feature = "serial-ascii")]
use super::pool::server_pool_allocate_serial_ascii;
#[cfg(feature = "serial-rtu")]
use super::pool::server_pool_allocate_serial_rtu;
use super::{
    app::CServerApp,
    callbacks::MbusServerHandlers,
    config::MbusServerConfig,
    pool::{
        MBUS_INVALID_SERVER_ID, MbusServerId, server_pool_free, with_serial_server,
        with_serial_server_uniform,
    },
};

// ── mbus_serial_rtu_server_new ────────────────────────────────────────────────

/// Creates a new Modbus Serial RTU server and returns an opaque server ID.
///
/// # Parameters
/// - `transport` — Transport callbacks providing connect/disconnect/send/recv.
/// - `handlers`  — Application callback table.
/// - `config`    — Server configuration (slave address, timeout).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` on failure.
///
/// # Safety
/// Same requirements as `mbus_tcp_server_new`.
#[cfg(feature = "serial-rtu")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_rtu_server_new(
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

    let modbus_config = match server_config.rtu_modbus_config() {
        Ok(c) => c,
        Err(_) => return MBUS_INVALID_SERVER_ID,
    };

    let transport = CRtuTransport::new(transport_callbacks);
    let app = CServerApp::new(server_handlers);
    let resilience = server_config.resilience();

    let inner = ServerServices::<_, _, 1>::with_queue_depth(
        transport,
        app,
        modbus_config,
        unit_id,
        resilience,
    );

    match server_pool_allocate_serial_rtu(inner) {
        Ok(id) => id,
        Err(_) => MBUS_INVALID_SERVER_ID,
    }
}

// ── mbus_serial_ascii_server_new ──────────────────────────────────────────────

/// Creates a new Modbus Serial ASCII server and returns an opaque server ID.
///
/// # Parameters
/// - `transport` — Transport callbacks providing connect/disconnect/send/recv.
/// - `handlers`  — Application callback table.
/// - `config`    — Server configuration (slave address, timeout).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` on failure.
///
/// # Safety
/// Same requirements as `mbus_tcp_server_new`.
#[cfg(feature = "serial-ascii")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_ascii_server_new(
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

    let modbus_config = match server_config.ascii_modbus_config() {
        Ok(c) => c,
        Err(_) => return MBUS_INVALID_SERVER_ID,
    };

    let transport = CAsciiTransport::new(transport_callbacks);
    let app = CServerApp::new(server_handlers);
    let resilience = server_config.resilience();

    let inner = ServerServices::<_, _, 1>::with_queue_depth(
        transport,
        app,
        modbus_config,
        unit_id,
        resilience,
    );

    match server_pool_allocate_serial_ascii(inner) {
        Ok(id) => id,
        Err(_) => MBUS_INVALID_SERVER_ID,
    }
}

// ── mbus_serial_server_free ───────────────────────────────────────────────────

/// Destroys a Serial server and releases its pool slot.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_free(id: MbusServerId) {
    server_pool_free(id);
}

// ── mbus_serial_server_connect ────────────────────────────────────────────────

/// Opens the serial server's transport.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_connect(id: MbusServerId) -> MbusStatusCode {
    match with_serial_server_uniform!(id, |srv| srv.connect().map_err(MbusStatusCode::from)) {
        Ok(Ok(())) => MbusStatusCode::MbusOk,
        Ok(Err(e)) => e,
        Err(e) => e,
    }
}

// ── mbus_serial_server_disconnect ─────────────────────────────────────────────

/// Closes the serial server's transport.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_disconnect(id: MbusServerId) -> MbusStatusCode {
    match with_serial_server_uniform!(id, |srv| {
        srv.disconnect();
    }) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => e,
    }
}

// ── mbus_serial_server_poll ───────────────────────────────────────────────────

/// Drives the serial server state machine for one iteration.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_poll(id: MbusServerId) -> MbusStatusCode {
    match with_serial_server_uniform!(id, |srv| {
        srv.poll();
    }) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => e,
    }
}

// ── mbus_serial_server_is_connected ───────────────────────────────────────────

/// Returns `true` if the serial server's transport is connected.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_is_connected(id: MbusServerId) -> bool {
    with_serial_server(id, |srv| srv.is_connected(), |srv| srv.is_connected()).unwrap_or(false)
}

// ── mbus_serial_server_pending_request_count ──────────────────────────────────

/// Returns the number of requests waiting in the priority queue.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_pending_request_count(id: MbusServerId) -> usize {
    with_serial_server(
        id,
        |srv| srv.pending_request_count(),
        |srv| srv.pending_request_count(),
    )
    .unwrap_or(0)
}

// ── mbus_serial_server_pending_response_count─────────────────────────────────

/// Returns the number of responses waiting for retry.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_pending_response_count(id: MbusServerId) -> usize {
    with_serial_server(
        id,
        |srv| srv.pending_response_count(),
        |srv| srv.pending_response_count(),
    )
    .unwrap_or(0)
}
