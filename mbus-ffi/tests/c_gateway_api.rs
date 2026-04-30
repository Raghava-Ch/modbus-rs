//! Smoke tests for the C gateway API.
//!
//! These tests do NOT exercise an actual transport — they only verify that
//! invalid handles and NULL pointers are rejected and that the public symbols
//! are present and linkable. End-to-end transport behaviour is validated by
//! the C example in `examples/c_gateway_demo/`.

#![cfg(feature = "c-gateway")]

use core::ffi::c_void;
use mbus_ffi::c::error::MbusStatusCode;
use mbus_ffi::c::gateway::{
    mbus_gateway_add_downstream, mbus_gateway_add_range_route, mbus_gateway_add_unit_route,
    mbus_gateway_free, mbus_gateway_new, mbus_gateway_poll, MbusGatewayId,
    MBUS_INVALID_GATEWAY_ID,
};
use mbus_ffi::c::transport::MbusTransportCallbacks;

// ── C lock hooks (no-op for tests; tests are single-threaded) ────────────────

#[unsafe(no_mangle)]
extern "C" fn mbus_pool_lock() {}
#[unsafe(no_mangle)]
extern "C" fn mbus_pool_unlock() {}
#[unsafe(no_mangle)]
extern "C" fn mbus_gateway_lock(_id: MbusGatewayId) {}
#[unsafe(no_mangle)]
extern "C" fn mbus_gateway_unlock(_id: MbusGatewayId) {}

// Reuse client-pool lock symbols too if linked against c feature; since this
// test only enables c-gateway, the client pool symbols are NOT referenced.

// ── Stub transport callbacks (never invoked in these tests) ──────────────────

unsafe extern "C" fn stub_connect(_u: *mut c_void) -> MbusStatusCode {
    MbusStatusCode::MbusOk
}
unsafe extern "C" fn stub_disconnect(_u: *mut c_void) -> MbusStatusCode {
    MbusStatusCode::MbusOk
}
unsafe extern "C" fn stub_send(_d: *const u8, _l: u16, _u: *mut c_void) -> MbusStatusCode {
    MbusStatusCode::MbusOk
}
unsafe extern "C" fn stub_recv(
    _b: *mut u8,
    _c: u16,
    out: *mut u16,
    _u: *mut c_void,
) -> MbusStatusCode {
    unsafe { *out = 0 };
    MbusStatusCode::MbusErrTimeout
}
unsafe extern "C" fn stub_is_connected(_u: *mut c_void) -> u8 {
    1
}

fn stub_callbacks() -> MbusTransportCallbacks {
    MbusTransportCallbacks {
        userdata: core::ptr::null_mut(),
        on_connect: Some(stub_connect),
        on_disconnect: Some(stub_disconnect),
        on_send: Some(stub_send),
        on_recv: Some(stub_recv),
        on_is_connected: Some(stub_is_connected),
    }
}

#[test]
fn null_upstream_returns_null_pointer_error() {
    let mut id: MbusGatewayId = 0;
    let rc = unsafe { mbus_gateway_new(core::ptr::null(), core::ptr::null(), &mut id) };
    assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
}

#[test]
fn null_out_id_returns_null_pointer_error() {
    let cb = stub_callbacks();
    let rc = unsafe { mbus_gateway_new(&cb, core::ptr::null(), core::ptr::null_mut()) };
    assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
}

#[test]
fn missing_callback_field_returns_invalid_configuration() {
    let mut cb = stub_callbacks();
    cb.on_send = None;
    let mut id: MbusGatewayId = MBUS_INVALID_GATEWAY_ID;
    let rc = unsafe { mbus_gateway_new(&cb, core::ptr::null(), &mut id) };
    assert_eq!(rc, MbusStatusCode::MbusErrInvalidConfiguration);
    assert_eq!(id, MBUS_INVALID_GATEWAY_ID);
}

#[test]
fn full_lifecycle_smoke() {
    let upstream = stub_callbacks();
    let mut id: MbusGatewayId = MBUS_INVALID_GATEWAY_ID;

    // Construct
    let rc = unsafe { mbus_gateway_new(&upstream, core::ptr::null(), &mut id) };
    assert_eq!(rc, MbusStatusCode::MbusOk);
    assert_ne!(id, MBUS_INVALID_GATEWAY_ID);

    // Routing without channels must be rejected
    let rc = mbus_gateway_add_unit_route(id, 1, 0);
    assert_eq!(rc, MbusStatusCode::MbusErrInvalidConfiguration);

    // Add a downstream channel
    let downstream = stub_callbacks();
    let mut ch: u16 = u16::MAX;
    let rc = unsafe { mbus_gateway_add_downstream(id, &downstream, &mut ch) };
    assert_eq!(rc, MbusStatusCode::MbusOk);
    assert_eq!(ch, 0);

    // Unit route now succeeds
    assert_eq!(mbus_gateway_add_unit_route(id, 1, 0), MbusStatusCode::MbusOk);
    // Duplicate unit fails
    assert_eq!(
        mbus_gateway_add_unit_route(id, 1, 0),
        MbusStatusCode::MbusErrInvalidConfiguration
    );
    // Range route succeeds
    assert_eq!(
        mbus_gateway_add_range_route(id, 10, 20, 0),
        MbusStatusCode::MbusOk
    );
    // Inverted range fails
    assert_eq!(
        mbus_gateway_add_range_route(id, 30, 20, 0),
        MbusStatusCode::MbusErrInvalidConfiguration
    );
    // Channel index out of range fails
    assert_eq!(
        mbus_gateway_add_unit_route(id, 5, 99),
        MbusStatusCode::MbusErrInvalidConfiguration
    );

    // Poll should succeed (stub recv returns Timeout → mapped to Ok by poll)
    let rc = mbus_gateway_poll(id);
    assert_eq!(rc, MbusStatusCode::MbusOk);

    // Free
    assert_eq!(mbus_gateway_free(id), MbusStatusCode::MbusOk);
    // Double free fails
    assert_eq!(
        mbus_gateway_free(id),
        MbusStatusCode::MbusErrInvalidClientId
    );
}

#[test]
fn invalid_id_rejected() {
    assert_eq!(
        mbus_gateway_poll(MBUS_INVALID_GATEWAY_ID),
        MbusStatusCode::MbusErrInvalidClientId
    );
    assert_eq!(
        mbus_gateway_free(MBUS_INVALID_GATEWAY_ID),
        MbusStatusCode::MbusErrInvalidClientId
    );
}
