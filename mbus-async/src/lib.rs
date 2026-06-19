#![warn(missing_docs)]

//! Async facade for the Modbus client and server stacks.
//!
//! # ⚠️ Obsolete Notice
//! The `mbus-async` crate is obsolete and consolidated into `mbus_server_async` and `mbus_client_async`.
//! `mbus-async` will be removed in the near future. Please migrate to using `mbus_server_async` and `mbus_client_async` directly.
//!
//! This crate re-exports its public API from internal submodules.
//! The full implementation lives in internal module files.

pub mod client;
#[cfg(any(
    feature = "server-tcp",
    feature = "server-serial",
    target_arch = "wasm32"
))]
pub mod server;

#[cfg(any(feature = "server-tcp", feature = "server-serial"))]
pub use mbus_macros::async_modbus_app;

pub use client::*;
