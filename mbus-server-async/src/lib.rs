#![warn(missing_docs)]

//! Async Modbus server facade crate.
//!
//! This crate re-exports the async server API from `mbus-async::server`.
//! It exists to provide a role-focused crate name for users that prefer
//! direct crate dependencies over the umbrella `modbus-rs` package.

pub use mbus_async::server::*;
