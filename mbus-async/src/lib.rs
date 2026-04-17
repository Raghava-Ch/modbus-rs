#![warn(missing_docs)]

//! Async facade for the Modbus client and server stacks.
//!
//! This crate re-exports its public API from internal submodules.
//! The full implementation lives in internal module files.

pub mod client;
#[cfg(any(feature = "server-tcp", feature = "server-serial"))]
pub mod server;

pub use client::*;
