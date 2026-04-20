//! Native C FFI bindings for the Modbus stack.
//!
//! Sub-modules:
//! - [`client`] — TCP and Serial (RTU/ASCII) client API
//! - [`server`] — TCP and Serial (RTU/ASCII) server API (feature `c-server`)

// ── Sub-modules ──────────────────────────────────────────────────────────────

pub mod client;

#[cfg(feature = "c-server")]
pub mod server;

// ── Module re-exports (keep `crate::c::error` / `crate::c::transport` paths) ─

pub use client::error;
pub use client::transport;

// ── Public re-exports ─────────────────────────────────────────────────────────

pub use client::MbusStatusCode;
pub use client::{MBUS_INVALID_CLIENT_ID, MbusClientId};
