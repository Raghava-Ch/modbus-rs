//! Native C FFI bindings for the Modbus client stack.
//!
//! This module provides a complete, `no_std`-compatible C API for creating
//! and operating Modbus TCP and Serial (RTU/ASCII) clients.
//!
//! # Design
//!
//! - **Static client pool**: Clients are stored in a fixed-capacity static
//!   pool (sized via the `MBUS_MAX_CLIENTS` environment variable at build time,
//!   default = 1).
//! - **ID-based API**: C code receives an opaque `MbusClientId` (u8) and uses
//!   it for all subsequent operations. No Rust pointers are exposed.
//! - **Zero heap allocation**: Everything is `core`-only + `heapless`.
//! - **Callback-driven**: Responses are delivered via C function-pointer
//!   callbacks registered at client creation time.

// ── Sub-modules ──────────────────────────────────────────────────────────────

pub mod error;
pub mod pool;
pub mod transport;
pub mod callbacks;
pub mod config;
pub mod app;
pub mod models;
pub mod tcp_client;
pub mod serial_client;

#[cfg(feature = "coils")]
pub mod coils;
#[cfg(feature = "registers")]
pub mod registers;
#[cfg(feature = "discrete-inputs")]
pub mod discrete_inputs;
#[cfg(feature = "fifo")]
pub mod fifo;
#[cfg(feature = "file-record")]
pub mod file_record;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;

// ── Re-exports ───────────────────────────────────────────────────────────────

pub use error::MbusStatusCode;
pub use pool::{MbusClientId, MBUS_INVALID_CLIENT_ID};
