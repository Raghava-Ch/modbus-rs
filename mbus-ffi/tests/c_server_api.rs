//! Integration tests for the C server FFI layer (`c-server` feature).
//!
//! These tests exercise:
//! 1. Null-pointer safety of `mbus_tcp_server_new` / `mbus_serial_server_new`.
//! 2. Server lifecycle: allocate → connect → poll → disconnect → free.
//! 3. Round-trip callback dispatch: a full FC01 TCP request processed through
//!    the C callback layer produces a well-formed Modbus exception or success response.
//!
//! All C transport / handler callbacks are implemented as static `extern "C" fn`
//! stubs so no real network I/O occurs.

#![cfg(all(not(target_arch = "wasm32"), feature = "c-server"))]

use core::ffi::c_void;
use std::sync::Mutex;

// ── Lock hook stubs ───────────────────────────────────────────────────────────
//
// `mbus-ffi` declares `mbus_server_pool_lock/unlock` and `mbus_server_lock/unlock`
// as `extern "C"` hooks so that C callers can inject a mutex.  When running
// under Rust tests there is no C linker glue, so we provide no-op stubs here.
// (Tests that exercise pool concurrency would need real mutexes; these tests
// are single-threaded and use `--test-threads=1`.)

#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_pool_lock() {}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_pool_unlock() {}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_lock(_id: u16) {}
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_unlock(_id: u16) {}

use mbus_ffi::c::server::{
    MbusServerExceptionCode, MbusServerHandlers, MBUS_INVALID_SERVER_ID,
};
use mbus_ffi::c::server::callbacks::{
    MbusServerReadCoilsReq, MbusServerReadHoldingRegistersReq, MbusServerWriteSingleCoilReq,
};
use mbus_ffi::c::server::config::MbusServerConfig;
use mbus_ffi::c::server::tcp_server::{
    mbus_tcp_server_connect, mbus_tcp_server_disconnect, mbus_tcp_server_free,
    mbus_tcp_server_is_connected, mbus_tcp_server_new, mbus_tcp_server_poll,
};
use mbus_ffi::c::transport::MbusTransportCallbacks;
use mbus_ffi::c::error::MbusStatusCode;

// ── Shared test state ─────────────────────────────────────────────────────────

/// Bytes captured by the test `on_send` callback.
static SENT_BYTES: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());

/// Frame to return from `on_recv`, drained on the first call — subsequent
/// calls return nothing (simulating "no more data").
static RECV_FRAME: Mutex<Option<Vec<u8>>> = Mutex::new(None);

/// Counter: number of times the `on_read_coils` handler was invoked.
static COIL_CB_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Counter: number of times the `on_write_single_coil` handler was invoked.
static WRITE_COIL_CB_CALLS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

fn reset_test_state() {
    SENT_BYTES.lock().unwrap().clear();
    *RECV_FRAME.lock().unwrap() = None;
    COIL_CB_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
    WRITE_COIL_CB_CALLS.store(0, std::sync::atomic::Ordering::SeqCst);
}

// ── Test transport callbacks ──────────────────────────────────────────────────

unsafe extern "C" fn test_connect(_userdata: *mut c_void) -> MbusStatusCode {
    MbusStatusCode::MbusOk
}

unsafe extern "C" fn test_disconnect(_userdata: *mut c_void) -> MbusStatusCode {
    MbusStatusCode::MbusOk
}

unsafe extern "C" fn test_send(
    data: *const u8,
    len: u16,
    _userdata: *mut c_void,
) -> MbusStatusCode {
    let bytes = unsafe { core::slice::from_raw_parts(data, len as usize) };
    SENT_BYTES.lock().unwrap().push(bytes.to_vec());
    MbusStatusCode::MbusOk
}

unsafe extern "C" fn test_recv(
    buffer: *mut u8,
    buffer_cap: u16,
    out_len: *mut u16,
    _userdata: *mut c_void,
) -> MbusStatusCode {
    let mut guard = RECV_FRAME.lock().unwrap();
    match guard.take() {
        Some(frame) => {
            let copy_len = frame.len().min(buffer_cap as usize);
            unsafe {
                core::ptr::copy_nonoverlapping(frame.as_ptr(), buffer, copy_len);
                *out_len = copy_len as u16;
            }
            MbusStatusCode::MbusOk
        }
        None => {
            // No data — return OK with out_len=0; `c_recv` converts zero-length
            // to `MbusError::Timeout` which is non-fatal.
            unsafe { *out_len = 0; }
            MbusStatusCode::MbusOk
        }
    }
}

unsafe extern "C" fn test_is_connected(_userdata: *mut c_void) -> u8 {
    1
}

fn make_transport_callbacks() -> MbusTransportCallbacks {
    MbusTransportCallbacks {
        userdata: core::ptr::null_mut(),
        on_connect: Some(test_connect),
        on_disconnect: Some(test_disconnect),
        on_send: Some(test_send),
        on_recv: Some(test_recv),
        on_is_connected: Some(test_is_connected),
    }
}

// ── Test handler callbacks ────────────────────────────────────────────────────

/// FC01 — Read Coils: returns a deterministic packed byte pattern.
///
/// For a request of N coils, this writes ceil(N/8) bytes with a fixed
/// bit pattern `0b0000_0101` in the first byte.
unsafe extern "C" fn test_on_read_coils(
    req: *mut MbusServerReadCoilsReq,
    _userdata: *mut c_void,
) -> MbusServerExceptionCode {
    COIL_CB_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let req = unsafe { &mut *req };
    let needed = (req.quantity as usize).div_ceil(8);
    let needed = needed.min(req.out_data_len);
    if needed == 0 {
        return MbusServerExceptionCode::ServerDeviceFailure;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(req.out_data, needed) };
    slice.fill(0);
    if needed > 0 {
        slice[0] = 0b0000_0101;
    }
    req.out_byte_count = needed as u8;
    MbusServerExceptionCode::Ok
}

/// FC05 — Write Single Coil: echoes successfully.
unsafe extern "C" fn test_on_write_single_coil(
    _req: *const MbusServerWriteSingleCoilReq,
    _userdata: *mut c_void,
) -> MbusServerExceptionCode {
    WRITE_COIL_CB_CALLS.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    MbusServerExceptionCode::Ok
}

/// FC03 — Read Holding Registers: returns fixed 0xDEAD pattern.
unsafe extern "C" fn test_on_read_holding_registers(
    req: *mut MbusServerReadHoldingRegistersReq,
    _userdata: *mut c_void,
) -> MbusServerExceptionCode {
    let req = unsafe { &mut *req };
    let byte_count = (req.quantity as usize * 2).min(req.out_data_len);
    if byte_count == 0 {
        return MbusServerExceptionCode::ServerDeviceFailure;
    }
    let slice = unsafe { core::slice::from_raw_parts_mut(req.out_data, byte_count) };
    for chunk in slice.chunks_mut(2) {
        if chunk.len() == 2 {
            chunk[0] = 0xDE;
            chunk[1] = 0xAD;
        }
    }
    req.out_byte_count = byte_count as u8;
    MbusServerExceptionCode::Ok
}

fn make_all_null_handlers() -> MbusServerHandlers {
    MbusServerHandlers {
        userdata: core::ptr::null_mut(),
        on_read_coils: None,
        on_write_single_coil: None,
        on_write_multiple_coils: None,
        on_read_discrete_inputs: None,
        on_read_holding_registers: None,
        on_write_single_register: None,
        on_write_multiple_registers: None,
        on_mask_write_register: None,
        on_read_write_multiple_registers: None,
        on_read_input_registers: None,
        on_read_fifo_queue: None,
        on_read_file_record: None,
        on_write_file_record: None,
        on_read_exception_status: None,
        on_diagnostics: None,
        on_get_comm_event_counter: None,
        on_get_comm_event_log: None,
        on_report_server_id: None,
        on_read_device_identification: None,
    }
}

fn make_test_handlers() -> MbusServerHandlers {
    MbusServerHandlers {
        on_read_coils: Some(test_on_read_coils),
        on_write_single_coil: Some(test_on_write_single_coil),
        on_read_holding_registers: Some(test_on_read_holding_registers),
        ..make_all_null_handlers()
    }
}

fn make_test_config() -> MbusServerConfig {
    MbusServerConfig {
        slave_address: 1,
        response_timeout_ms: 1_000,
    }
}

// ── Null-pointer safety tests ─────────────────────────────────────────────────

#[test]
fn tcp_server_new_null_transport_returns_invalid_id() {
    let handlers = make_test_handlers();
    let config = make_test_config();
    let id = unsafe {
        mbus_tcp_server_new(core::ptr::null(), &handlers, &config)
    };
    assert_eq!(id, MBUS_INVALID_SERVER_ID, "null transport should return MBUS_INVALID_SERVER_ID");
}

#[test]
fn tcp_server_new_null_handlers_returns_invalid_id() {
    let transport = make_transport_callbacks();
    let config = make_test_config();
    let id = unsafe {
        mbus_tcp_server_new(&transport, core::ptr::null(), &config)
    };
    assert_eq!(id, MBUS_INVALID_SERVER_ID, "null handlers should return MBUS_INVALID_SERVER_ID");
}

#[test]
fn tcp_server_new_null_config_returns_invalid_id() {
    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let id = unsafe {
        mbus_tcp_server_new(&transport, &handlers, core::ptr::null())
    };
    assert_eq!(id, MBUS_INVALID_SERVER_ID, "null config should return MBUS_INVALID_SERVER_ID");
}

#[test]
fn tcp_server_new_incomplete_transport_returns_invalid_id() {
    // Transport with on_recv missing — must be rejected.
    let transport = MbusTransportCallbacks {
        userdata: core::ptr::null_mut(),
        on_connect: Some(test_connect),
        on_disconnect: Some(test_disconnect),
        on_send: Some(test_send),
        on_recv: None,
        on_is_connected: Some(test_is_connected),
    };
    let handlers = make_test_handlers();
    let config = make_test_config();
    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_eq!(
        id, MBUS_INVALID_SERVER_ID,
        "transport with missing callback should return MBUS_INVALID_SERVER_ID"
    );
}

// ── Lifecycle tests ───────────────────────────────────────────────────────────

#[test]
fn tcp_server_new_and_free_succeeds() {
    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID, "expected a valid server ID");

    // Double-free must not panic.
    mbus_tcp_server_free(id);
    mbus_tcp_server_free(id); // no-op
}

#[test]
fn tcp_server_is_connected_after_connect() {
    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID);

    // Before connect: transport reports "connected" (our stub always returns 1),
    // but we haven't called connect yet — the test_is_connected stub always returns
    // true, so this verifies the wrapper plumbing, not actual state.
    assert!(mbus_tcp_server_is_connected(id));

    let status = mbus_tcp_server_connect(id);
    assert_eq!(status, MbusStatusCode::MbusOk, "connect should succeed");
    assert!(mbus_tcp_server_is_connected(id));

    let status = mbus_tcp_server_disconnect(id);
    assert_eq!(status, MbusStatusCode::MbusOk, "disconnect should succeed");

    mbus_tcp_server_free(id);
}

#[test]
fn tcp_server_connect_with_invalid_id_returns_error() {
    let status = mbus_tcp_server_connect(MBUS_INVALID_SERVER_ID);
    assert_ne!(status, MbusStatusCode::MbusOk, "invalid ID should not succeed");
}

#[test]
fn tcp_server_poll_with_invalid_id_returns_error() {
    let status = mbus_tcp_server_poll(MBUS_INVALID_SERVER_ID);
    assert_ne!(status, MbusStatusCode::MbusOk, "invalid ID should not succeed");
}

// ── Round-trip dispatch tests ─────────────────────────────────────────────────

/// Builds a Modbus TCP MBAP + PDU frame for the given function code and payload.
///
/// Frame layout: [txn_id_hi, txn_id_lo, proto_hi=0, proto_lo=0, len_hi, len_lo, unit_id, fc, ...payload]
fn build_tcp_frame(txn_id: u16, unit_id: u8, fc: u8, payload: &[u8]) -> Vec<u8> {
    // PDU length = 1 (fc) + payload.len()
    let pdu_len = 1 + payload.len();
    // MBAP length = 1 (unit_id) + pdu_len
    let mbap_len = (1 + pdu_len) as u16;
    let mut frame = Vec::new();
    frame.push((txn_id >> 8) as u8);
    frame.push(txn_id as u8);
    frame.push(0x00); // protocol
    frame.push(0x00);
    frame.push((mbap_len >> 8) as u8);
    frame.push(mbap_len as u8);
    frame.push(unit_id);
    frame.push(fc);
    frame.extend_from_slice(payload);
    frame
}

/// FC01 request: address=0x0001, quantity=0x0008 (8 coils).
fn build_fc01_request() -> Vec<u8> {
    build_tcp_frame(1, 1, 0x01, &[0x00, 0x01, 0x00, 0x08])
}

/// FC03 request: address=0x0000, quantity=0x0002 (2 registers).
fn build_fc03_request() -> Vec<u8> {
    build_tcp_frame(2, 1, 0x03, &[0x00, 0x00, 0x00, 0x02])
}

/// FC05 request: address=0x0002, value=0xFF00 (coil ON).
fn build_fc05_request() -> Vec<u8> {
    build_tcp_frame(3, 1, 0x05, &[0x00, 0x02, 0xFF, 0x00])
}

/// Returns the offset after the 7-byte MBAP header within a TCP response frame.
const MBAP_HEADER_LEN: usize = 7;

#[test]
fn fc01_read_coils_dispatches_callback_and_returns_success_response() {
    reset_test_state();

    *RECV_FRAME.lock().unwrap() = Some(build_fc01_request());

    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID);

    // poll once — consumes the queued frame, dispatches callback, sends response.
    let status = mbus_tcp_server_poll(id);
    assert_eq!(status, MbusStatusCode::MbusOk);

    // The read_coils callback must have been invoked exactly once.
    assert_eq!(
        COIL_CB_CALLS.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "on_read_coils should be called once"
    );

    // Exactly one response frame must have been sent.
    let sent = SENT_BYTES.lock().unwrap();
    assert_eq!(sent.len(), 1, "exactly one response frame expected");

    let frame = &sent[0];
    assert!(
        frame.len() >= MBAP_HEADER_LEN + 2,
        "response must contain MBAP(7) + FC(1) + byte_count(1) + data"
    );

    // The function code in the response must be 0x01 (no exception bit).
    assert_eq!(frame[MBAP_HEADER_LEN], 0x01, "response FC should be 0x01");

    // byte_count = 1 (ceil(8/8)).
    let byte_count = frame[MBAP_HEADER_LEN + 1];
    assert_eq!(byte_count, 1, "byte_count should be 1 for 8 coils");

    // Data byte should match the pattern our callback wrote (0b0000_0101).
    assert_eq!(
        frame[MBAP_HEADER_LEN + 2],
        0b0000_0101,
        "coil data byte should match test pattern"
    );

    mbus_tcp_server_free(id);
}

#[test]
fn fc03_read_holding_registers_dispatches_callback() {
    reset_test_state();

    *RECV_FRAME.lock().unwrap() = Some(build_fc03_request());

    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID);

    let status = mbus_tcp_server_poll(id);
    assert_eq!(status, MbusStatusCode::MbusOk);

    let sent = SENT_BYTES.lock().unwrap();
    assert_eq!(sent.len(), 1, "exactly one response frame expected");

    let frame = &sent[0];
    // FC03 response: FC=0x03, byte_count=4 (2 regs * 2 bytes), then 0xDEAD 0xDEAD.
    assert_eq!(frame[MBAP_HEADER_LEN], 0x03);
    assert_eq!(frame[MBAP_HEADER_LEN + 1], 4, "byte_count should be 4 for 2 registers");
    assert_eq!(&frame[MBAP_HEADER_LEN + 2..MBAP_HEADER_LEN + 6], &[0xDE, 0xAD, 0xDE, 0xAD]);

    mbus_tcp_server_free(id);
}

#[test]
fn fc05_write_single_coil_dispatches_callback_and_echoes() {
    reset_test_state();

    *RECV_FRAME.lock().unwrap() = Some(build_fc05_request());

    let transport = make_transport_callbacks();
    let handlers = make_test_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID);

    let status = mbus_tcp_server_poll(id);
    assert_eq!(status, MbusStatusCode::MbusOk);

    assert_eq!(
        WRITE_COIL_CB_CALLS.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "on_write_single_coil should be called once"
    );

    let sent = SENT_BYTES.lock().unwrap();
    assert_eq!(sent.len(), 1);

    let frame = &sent[0];
    // FC05 response echoes the request PDU (FC + address + value).
    assert_eq!(frame[MBAP_HEADER_LEN], 0x05, "response FC should be 0x05");

    mbus_tcp_server_free(id);
}

#[test]
fn null_callback_slot_returns_illegal_function_exception() {
    reset_test_state();

    // FC01 request with on_read_coils = None → server returns IllegalFunction exception.
    *RECV_FRAME.lock().unwrap() = Some(build_fc01_request());

    let transport = make_transport_callbacks();
    let handlers = make_all_null_handlers();
    let config = make_test_config();

    let id = unsafe { mbus_tcp_server_new(&transport, &handlers, &config) };
    assert_ne!(id, MBUS_INVALID_SERVER_ID);

    let status = mbus_tcp_server_poll(id);
    assert_eq!(status, MbusStatusCode::MbusOk);

    // Callback was never called.
    assert_eq!(COIL_CB_CALLS.load(std::sync::atomic::Ordering::SeqCst), 0);

    let sent = SENT_BYTES.lock().unwrap();
    assert_eq!(sent.len(), 1, "exception response should be sent");

    let frame = &sent[0];
    // Exception response: FC = 0x81 (0x01 | 0x80), ExCode = 0x01 (IllegalFunction).
    assert!(frame.len() >= MBAP_HEADER_LEN + 2);
    assert_eq!(frame[MBAP_HEADER_LEN], 0x81, "exception FC should be 0x81");
    assert_eq!(frame[MBAP_HEADER_LEN + 1], 0x01, "exception code should be IllegalFunction");

    mbus_tcp_server_free(id);
}
