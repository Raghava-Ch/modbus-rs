//! Public client constructors and request methods exposed to Go via cgo.

pub mod serial;
pub mod tcp;

pub use tcp::*;
