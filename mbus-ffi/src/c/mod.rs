//! Native C FFI bindings for the Modbus stack.
//!
//! Sub-modules:
//! - [`error`] / [`transport`] — shared types (always compiled)
//! - [`client`] — TCP and Serial client API (feature `c`)
//! - [`server`] — TCP and Serial server API (feature `c-server`)

// ── Shared types (no feature gate — server also needs these) ─────────────────

pub mod error;
pub mod transport;

// ── Client bindings ───────────────────────────────────────────────────────────

#[cfg(feature = "c")]
pub mod client;

// ── Server bindings ───────────────────────────────────────────────────────────

#[cfg(feature = "c-server")]
pub mod server;

#[cfg(feature = "c-server")]
pub mod server_gen;

// ── Gateway bindings ──────────────────────────────────────────────────────────

#[cfg(feature = "c-gateway")]
pub mod gateway;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use error::MbusStatusCode;

#[cfg(feature = "c")]
pub use client::{MBUS_INVALID_CLIENT_ID, MbusClientId};

#[cfg(feature = "c-gateway")]
pub use gateway::{MBUS_INVALID_GATEWAY_ID, MbusGatewayId};
