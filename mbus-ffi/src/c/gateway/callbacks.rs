//! C-visible event callback table for gateway observability.
//!
//! Each function pointer is optional (`NULL` ⇒ silently ignored). All
//! callbacks are invoked synchronously from the calling thread of
//! [`mbus_gateway_poll`](super::gateway::mbus_gateway_poll), inside the
//! per-gateway lock. They must not call back into the gateway API on the
//! same instance, or a deadlock will result.

use core::ffi::c_void;

/// Observability callbacks delivered by the gateway.
///
/// Pass a populated value to [`mbus_gateway_new`](super::gateway::mbus_gateway_new).
/// Any field set to `None` (`NULL` from C) is treated as a no-op.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct MbusGatewayCallbacks {
    /// Opaque user pointer threaded to every callback.
    pub userdata: *mut c_void,

    /// Called after a request has been forwarded to a downstream channel.
    pub on_forward: Option<
        unsafe extern "C" fn(
            session_id: u8,
            unit_id: u8,
            channel_idx: u16,
            userdata: *mut c_void,
        ),
    >,

    /// Called after a response has been delivered to the upstream client.
    pub on_response_returned: Option<
        unsafe extern "C" fn(session_id: u8, upstream_txn: u16, userdata: *mut c_void),
    >,

    /// Called when no downstream route was found for a unit ID.
    pub on_routing_miss:
        Option<unsafe extern "C" fn(session_id: u8, unit_id: u8, userdata: *mut c_void)>,

    /// Called when a downstream device failed to respond before timeout.
    pub on_downstream_timeout: Option<
        unsafe extern "C" fn(session_id: u8, internal_txn: u16, userdata: *mut c_void),
    >,

    /// Called when the upstream session disconnects (transport-level error).
    pub on_upstream_disconnect:
        Option<unsafe extern "C" fn(session_id: u8, userdata: *mut c_void)>,
}

// SAFETY: The C application is responsible for ensuring the userdata pointer
// is safely shareable across threads (or for serializing all access via the
// extern lock hooks).
unsafe impl Send for MbusGatewayCallbacks {}
unsafe impl Sync for MbusGatewayCallbacks {}

impl Default for MbusGatewayCallbacks {
    fn default() -> Self {
        Self {
            userdata: core::ptr::null_mut(),
            on_forward: None,
            on_response_returned: None,
            on_routing_miss: None,
            on_downstream_timeout: None,
            on_upstream_disconnect: None,
        }
    }
}
