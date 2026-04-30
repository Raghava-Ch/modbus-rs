//! Public `extern "C"` gateway API.
//!
//! Lifecycle:
//!
//! 1. [`mbus_gateway_new`] — create a TCP↔TCP gateway and return its handle.
//! 2. [`mbus_gateway_add_downstream`] — register one or more downstream channels.
//! 3. [`mbus_gateway_add_unit_route`] / [`mbus_gateway_add_range_route`] — populate routing.
//! 4. [`mbus_gateway_poll`] — call repeatedly to drive request/response cycles.
//! 5. [`mbus_gateway_free`] — release the handle.

use crate::c::error::MbusStatusCode;
use crate::c::transport::{validate_transport_callbacks, CTcpTransport, MbusTransportCallbacks};

use mbus_gateway::{DownstreamChannel, GatewayServices};

use super::callbacks::MbusGatewayCallbacks;
use super::event_adapter::CGatewayEventAdapter;
use super::pool::{
    pool_allocate, pool_free, with_gateway, GatewayInner, MAX_DOWNSTREAM_CHANNELS,
};
use super::routing::CGatewayRouter;

pub use super::pool::{MBUS_INVALID_GATEWAY_ID, MbusGatewayId};

/// Create a new TCP↔TCP gateway.
///
/// `upstream` describes the C-provided upstream transport callbacks.
/// `events` may be `NULL` to disable observability callbacks.
///
/// On success writes the new handle into `*out_id` and returns
/// [`MbusStatusCode::MbusOk`]. On failure leaves `*out_id` set to
/// [`MBUS_INVALID_GATEWAY_ID`].
///
/// # Safety
/// `upstream` and `out_id` must be non-NULL. `events` may be NULL. All
/// non-NULL pointers must point to valid memory for the duration of this
/// call. The function pointers in `upstream` and `events` must outlive the
/// gateway instance.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_new(
    upstream: *const MbusTransportCallbacks,
    events: *const MbusGatewayCallbacks,
    out_id: *mut MbusGatewayId,
) -> MbusStatusCode {
    if upstream.is_null() || out_id.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    // SAFETY: caller-validated above.
    unsafe { *out_id = MBUS_INVALID_GATEWAY_ID };

    // SAFETY: `upstream` is a non-null pointer to caller-owned memory.
    let upstream_cb = unsafe { core::ptr::read(upstream) };
    if !validate_transport_callbacks(&upstream_cb) {
        return MbusStatusCode::MbusErrInvalidConfiguration;
    }

    let event_cb = if events.is_null() {
        MbusGatewayCallbacks::default()
    } else {
        // SAFETY: caller-validated non-null.
        unsafe { core::ptr::read(events) }
    };

    let upstream_transport = CTcpTransport::new(upstream_cb);
    let router = CGatewayRouter::new();
    let event_adapter = CGatewayEventAdapter::new(event_cb);
    let services: GatewayInner = GatewayServices::new(upstream_transport, router, event_adapter);

    match pool_allocate(services) {
        Ok(id) => {
            // SAFETY: caller-validated non-null.
            unsafe { *out_id = id };
            MbusStatusCode::MbusOk
        }
        Err(status) => status,
    }
}

/// Register a TCP downstream channel.
///
/// Channels are indexed in registration order starting at 0. The new index
/// is written to `*out_channel_idx`.
///
/// # Safety
/// `callbacks` and `out_channel_idx` must be non-NULL and point to valid
/// memory for the duration of this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_add_downstream(
    id: MbusGatewayId,
    callbacks: *const MbusTransportCallbacks,
    out_channel_idx: *mut u16,
) -> MbusStatusCode {
    if callbacks.is_null() || out_channel_idx.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    // SAFETY: caller-validated.
    let cb = unsafe { core::ptr::read(callbacks) };
    if !validate_transport_callbacks(&cb) {
        return MbusStatusCode::MbusErrInvalidConfiguration;
    }

    let result = with_gateway(id, |gw| {
        let transport = CTcpTransport::new(cb);
        let channel = DownstreamChannel::new(transport);
        match gw.add_downstream(channel) {
            Ok(()) => {
                let new_idx = (gw.downstream_count() - 1) as u16;
                Ok(new_idx)
            }
            Err(e) => Err(MbusStatusCode::from(e)),
        }
    });

    match result {
        Ok(Ok(idx)) => {
            // SAFETY: caller-validated.
            unsafe { *out_channel_idx = idx };
            MbusStatusCode::MbusOk
        }
        Ok(Err(status)) => status,
        Err(status) => status,
    }
}

/// Register an exact unit-ID → channel-index route.
///
/// Returns [`MbusStatusCode::MbusErrInvalidConfiguration`] if the routing
/// table is full, the unit is already registered, or `channel_idx` is out
/// of range.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_gateway_add_unit_route(
    id: MbusGatewayId,
    unit_id: u8,
    channel_idx: u16,
) -> MbusStatusCode {
    if (channel_idx as usize) >= MAX_DOWNSTREAM_CHANNELS {
        return MbusStatusCode::MbusErrInvalidConfiguration;
    }
    let result = with_gateway(id, |gw| {
        if (channel_idx as usize) >= gw.downstream_count() {
            return MbusStatusCode::MbusErrInvalidConfiguration;
        }
        if gw.router_mut().add_unit(unit_id, channel_idx as usize) {
            MbusStatusCode::MbusOk
        } else {
            MbusStatusCode::MbusErrInvalidConfiguration
        }
    });
    result.unwrap_or_else(|s| s)
}

/// Register a contiguous unit-ID range → channel-index route. `unit_min`
/// and `unit_max` are inclusive.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_gateway_add_range_route(
    id: MbusGatewayId,
    unit_min: u8,
    unit_max: u8,
    channel_idx: u16,
) -> MbusStatusCode {
    if (channel_idx as usize) >= MAX_DOWNSTREAM_CHANNELS {
        return MbusStatusCode::MbusErrInvalidConfiguration;
    }
    let result = with_gateway(id, |gw| {
        if (channel_idx as usize) >= gw.downstream_count() {
            return MbusStatusCode::MbusErrInvalidConfiguration;
        }
        if gw
            .router_mut()
            .add_range(unit_min, unit_max, channel_idx as usize)
        {
            MbusStatusCode::MbusOk
        } else {
            MbusStatusCode::MbusErrInvalidConfiguration
        }
    });
    result.unwrap_or_else(|s| s)
}

/// Drive one poll cycle of the gateway. See
/// [`GatewayServices::poll`](mbus_gateway::GatewayServices::poll) for the
/// semantics.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_gateway_poll(id: MbusGatewayId) -> MbusStatusCode {
    let result = with_gateway(id, |gw| match gw.poll() {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    });
    result.unwrap_or_else(|s| s)
}

/// Free the gateway at `id`. Returns [`MbusStatusCode::MbusOk`] on success
/// or [`MbusStatusCode::MbusErrInvalidClientId`] if the handle is invalid.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_gateway_free(id: MbusGatewayId) -> MbusStatusCode {
    if pool_free(id) {
        MbusStatusCode::MbusOk
    } else {
        MbusStatusCode::MbusErrInvalidClientId
    }
}
