#![warn(missing_docs)]

//! Async facade for the Modbus client stack.
//!
//! This crate re-exports its public API from internal submodules.
//! The full implementation lives in internal module files.

mod runtime;

pub use runtime::*;
