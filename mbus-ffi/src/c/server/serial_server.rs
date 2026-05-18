//! C API lifecycle functions for Modbus Serial servers (RTU and ASCII).

#![cfg(all(
    feature = "c-server",
    any(feature = "serial-rtu", feature = "serial-ascii")
))]
//!
//! Uses the same pool as TCP servers but a separate sub-pool with `QUEUE_DEPTH = 1`
//! because half-duplex serial allows only one request in flight at a time.
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
//!
//! # Error Returns (lifecycle functions)
//!
//! | Code | Meaning |
//! |------|---------|
//! | `MbusOk` | Operation succeeded. |
//! | `MbusErrConnectionFailed` | The `connect` transport callback returned an error. |
//! | `MbusErrConnectionClosed` | The `disconnect` callback (or recv path) signalled a closed bus. |
//! | `MbusErrInvalidClientId` | The `id` is `MBUS_INVALID_SERVER_ID` or refers to a freed slot. |
//!
//! # Modbus Serial Timing — Responsibility of the C Transport Layer
//!
//! The Modbus Serial Line specification (§2.5) defines two inter-frame silence periods
//! that the **C `recv` transport callback** is responsible for enforcing:
//!
//! ## t3.5 — Frame end delimiter (end-of-frame detection)
//!
//! A silence of **3.5 character times** on the bus signals the end of a complete RTU
//! frame. The `recv` callback must not return partial data; it must wait until either:
//! - a full frame has been received (t3.5 elapsed after the last byte), or
//! - a non-blocking timeout occurs (return `MBUS_ERR_TIMEOUT`).
//!
//! At baud rate B, one character time = 11 bits / B seconds:
//!
//! | Baud rate | 1 char time | t3.5 silence |
//! |-----------|-------------|--------------|
//! |  9 600    |  1 146 µs   |  4 010 µs    |
//! | 19 200    |    573 µs   |  2 005 µs    |
//! | 38 400    |    286 µs   |  1 003 µs    |
//! | 115 200   |     95 µs   |    334 µs    |
//!
//! For baud rates above 19 200, the Modbus spec allows rounding up to a fixed
//! minimum: **750 µs for t1.5** and **1 750 µs for t3.5**.
//!
//! ## t1.5 — Framing violation (inter-character gap)
//!
//! A silence of **1.5 character times** *inside* a frame (between consecutive bytes
//! of the same PDU) is a **framing error**. This indicates that the frame is corrupt
//! and must be discarded.
//!
//! **When a t1.5 violation is detected, the `recv` callback must return
//! `MBUS_ERR_FRAMING_ERROR` (`MbusErrFramingError`).** The server will then:
//!
//! 1. Discard all bytes accumulated so far in the receive buffer.
//! 2. Resume listening for the next valid frame start.
//! 3. **Not** disconnect the transport — the bus itself is still functional.
//!
//! ## Implementation guidance (C side)
//!
//! ```c
//! // Example: hardware-timer-based RTU recv callback
//! static MbusStatusCode my_serial_recv(uint8_t *buf, size_t *len, void *userdata) {
//!     // Receive bytes until t3.5 silence or buffer full.
//!     size_t received = 0;
//!     uint64_t last_byte_us = now_us();
//!
//!     while (1) {
//!         uint8_t byte;
//!         if (uart_read_byte(&byte, POLL_TIMEOUT_US)) {
//!             buf[received++] = byte;
//!             last_byte_us = now_us();
//!         } else {
//!             uint64_t silence_us = now_us() - last_byte_us;
//!             if (received > 0 && silence_us >= T35_US) {
//!                 // Full frame received — return it.
//!                 *len = received;
//!                 return MBUS_OK;
//!             }
//!             if (received == 0) {
//!                 return MBUS_ERR_TIMEOUT;   // No bytes at all yet.
//!             }
//!             if (silence_us >= T15_US && silence_us < T35_US) {
//!                 // Gap detected WITHIN a frame — framing error!
//!                 return MBUS_ERR_FRAMING_ERROR;
//!             }
//!         }
//!     }
//! }
//! ```
//!
//! ## Turnaround delay (server → master response)
//!
//! After a request is received and processed, the Modbus spec requires the slave to
//! wait at least **t3.5** before driving the bus with a response (to ensure the master
//! has released its driver). Configure this via `ResilienceConfig::turnaround_delay_us`.
//! When non-zero, `mbus_serial_server_poll` will defer `send` until that many
//! microseconds have elapsed since the last received byte (as measured by the clock
//! function supplied to `ResilienceConfig::clock_fn`).

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
///   The `recv` callback is responsible for t1.5/t3.5 silence detection (see
///   the module-level timing section).
/// - `handlers`  — Application callback table. `NULL` slots automatically
///   respond with `IllegalFunction` for that function code.
/// - `config`    — Server configuration (slave address, timeout).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` on failure.
///
/// `MBUS_INVALID_SERVER_ID` is returned when:
/// - Any pointer argument is `NULL`.
/// - Any required function pointer inside `transport` is `NULL`.
/// - `config.slave_address` is outside the valid range 1–247.
/// - The internal serial server pool is exhausted.
///
/// # Safety
/// - `transport`, `handlers`, and `config` must remain valid for the entire
///   lifetime of the server (until `mbus_serial_server_free` is called).
/// - All function pointers inside `transport` must be valid C functions.
/// - `handlers.userdata` must outlive the server.
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
/// ASCII mode uses LRC checksums and `:` / CR-LF delimiters instead of CRC and
/// silence-based framing, so t1.5/t3.5 inter-character timing is relaxed.
/// The `recv` callback may still return `MbusErrFramingError` if the ASCII frame
/// is malformed (e.g. an unexpected character between `:` and CR-LF).
///
/// # Parameters
/// - `transport` — Transport callbacks for the underlying serial port.
/// - `handlers`  — Application callback table.
/// - `config`    — Server configuration (slave address, timeout).
///
/// # Returns
/// A valid `MbusServerId` on success, or `MBUS_INVALID_SERVER_ID` on failure.
///
/// `MBUS_INVALID_SERVER_ID` is returned under the same conditions as
/// `mbus_serial_rtu_server_new` (null pointers, invalid slave address, pool full).
///
/// # Safety
/// Same requirements as `mbus_serial_rtu_server_new`.
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
///
/// The server's transport is **not** automatically disconnected. Call
/// `mbus_serial_server_disconnect` first if the port may still be open.
///
/// After this call, `id` becomes invalid and must not be reused.
///
/// # Error conditions (no return value)
/// This function is infallible — an invalid or already-freed `id` is silently
/// ignored to simplify cleanup paths.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_free(id: MbusServerId) {
    server_pool_free(id);
}

// ── mbus_serial_server_connect ────────────────────────────────────────────────

/// Opens the serial server's transport (e.g. opens and configures the UART).
///
/// Invokes the `connect` function-pointer from `MbusTransportCallbacks`.
///
/// # Returns
/// - `MbusOk` — Port opened successfully.
/// - `MbusErrConnectionFailed` — The `connect` callback returned a failure
///   (e.g. the serial port path does not exist, or permission denied).
/// - `MbusErrIoError` — A lower-level I/O error occurred while configuring
///   the port (baud rate, parity, stop bits).
/// - `MbusErrInvalidConfiguration` — The transport rejected the supplied
///   configuration (e.g. unsupported baud rate on this hardware).
/// - `MbusErrInvalidClientId` — `id` is not a valid serial server slot.
///
/// The server lock must be held while calling this function.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_connect(id: MbusServerId) -> MbusStatusCode {
    match with_serial_server_uniform!(id, |srv| srv.connect().map_err(MbusStatusCode::from)) {
        Ok(Ok(())) => MbusStatusCode::MbusOk,
        Ok(Err(e)) => e,
        Err(e) => e,
    }
}

// ── mbus_serial_server_disconnect ─────────────────────────────────────────────

/// Closes the serial server's transport (e.g. releases the UART).
///
/// Invokes the `disconnect` function-pointer from `MbusTransportCallbacks`.
/// Outstanding queued responses are discarded; the server pool slot remains
/// valid and can be reconnected with `mbus_serial_server_connect`.
///
/// # Returns
/// - `MbusOk` — Port closed (or was already closed).
/// - `MbusErrInvalidClientId` — `id` is not a valid serial server slot.
///
/// The server lock must be held while calling this function.
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
///
/// Must be called in a tight loop or cooperative scheduler task. Each call:
/// 1. **Response retry** — resends queued responses from previous failed sends.
/// 2. **Receive** — calls the `recv` transport callback once to collect bytes.
/// 3. **Frame assembly** — appends received bytes to the internal sliding window.
/// 4. **Dispatch** — when a complete RTU/ASCII frame is recognised, the matching
///    `MbusServerHandlers` callback is invoked.
/// 5. **Send** — transmits the response via the `send` transport callback.
///
/// # Returns
/// This function always returns `MbusOk` for normal poll-loop operation.
/// Transport-level events are handled internally:
///
/// | `recv` result | Server action |
/// |---------------|---------------|
/// | `MbusOk` | Bytes appended to the receive buffer; frame assembly continues. |
/// | `MbusErrTimeout` | No data available this poll; silently skipped. |
/// | `MbusErrFramingError` | **Timing violation detected** (see below). Receive buffer is cleared; server resumes listening. |
/// | `MbusErrIoError` / `MbusErrConnectionClosed` | Transport is marked disconnected. Call `mbus_serial_server_connect` to reopen. |
/// | `send` fails | Response is queued for retry on subsequent polls (up to `max_send_retries`). |
/// | CRC/parse error | One byte is discarded and the sliding window re-syncs. |
///
/// - `MbusErrInvalidClientId` — returned only when `id` is not a valid serial slot.
///
/// # Framing error handling
///
/// When the C `recv` callback detects a t1.5 inter-character gap violation (a
/// silence of 1.5 character times *inside* a frame), it must return
/// `MBUS_ERR_FRAMING_ERROR`.  The server will:
/// - Discard all bytes buffered so far for the current frame.
/// - **Not** disconnect — the bus is still usable.
/// - Resume waiting for the next valid frame on the next poll call.
///
/// This is the correct behaviour per Modbus Serial Line Specification §2.5.1.2.
///
/// The server lock must be held while calling this function.
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
///
/// Delegates to the `is_connected` function-pointer in `MbusTransportCallbacks`.
/// Returns `false` for an invalid or freed server ID.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_is_connected(id: MbusServerId) -> bool {
    with_serial_server(id, |srv| srv.is_connected(), |srv| srv.is_connected()).unwrap_or(false)
}

// ── mbus_serial_server_pending_request_count ──────────────────────────────────

/// Returns the number of requests waiting in the priority queue.
///
/// For serial servers the queue depth is fixed at 1 (half-duplex bus allows
/// only one request in flight), so this will always be 0 or 1.
/// Returns `0` for an invalid or freed server ID.
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

/// Returns the number of responses waiting for retry (failed sends).
///
/// A non-zero value means the `send` transport callback has failed at least once
/// and the server is holding the unsent frame for retry on the next poll.
/// Returns `0` for an invalid or freed server ID.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_server_pending_response_count(id: MbusServerId) -> usize {
    with_serial_server(
        id,
        |srv| srv.pending_response_count(),
        |srv| srv.pending_response_count(),
    )
    .unwrap_or(0)
}
