//! Native C FFI bindings for the Modbus gateway.
//!
//! All transports are **C-developer-provided** via `MbusTransportCallbacks`
//! — Rust never opens a socket or serial port. All locking is **C-developer-provided**
//! via the `extern "C"` hooks (`mbus_pool_lock` / `mbus_pool_unlock` reused from
//! the client pool, plus new `mbus_gateway_lock` / `mbus_gateway_unlock`
//! per-instance hooks).
//!
//! This module is strictly `no_std`.

pub mod callbacks;
pub mod event_adapter;
#[allow(clippy::module_inception)]
pub mod gateway;
pub mod pool;
pub mod routing;

pub use callbacks::MbusGatewayCallbacks;
pub use gateway::{
    mbus_gateway_add_downstream, mbus_gateway_add_range_route, mbus_gateway_add_unit_route,
    mbus_gateway_free, mbus_gateway_new, mbus_gateway_poll, MBUS_INVALID_GATEWAY_ID,
    MbusGatewayId,
};
