#![warn(missing_docs)]

//! Async Modbus client facade crate.
//!
//! This crate re-exports the async client API from `mbus-async::client`.
//! It exists to provide a role-focused crate name for users that prefer
//! direct crate dependencies over the umbrella `modbus-rs` package.

pub use mbus_async::client::*;
