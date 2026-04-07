//! Native C FFI bindings for the Modbus client stack.
//!
//! This module provides a complete, `no_std`-compatible C API for creating
//! and operating Modbus TCP and Serial (RTU/ASCII) clients.
//!
//! # Design
//!
//! - **Split typed static pools**: TCP clients occupy `tcp_slots[0..MAX_TCP_CLIENTS]`
//!   and Serial clients occupy `serial_slots[0..MAX_SERIAL_CLIENTS]`. Pool sizes
//!   are configured via `MBUS_MAX_TCP_CLIENTS` and `MBUS_MAX_SERIAL_CLIENTS`
//!   environment variables at build time (both default to 1).
//! - **ID-based API**: C code receives an opaque `MbusClientId` (u8). The MSB
//!   encodes the pool type (0 = TCP, 1 = Serial); the lower 7 bits are the slot
//!   index. `0xFF` is the reserved invalid sentinel.
//! - **Zero heap allocation**: Everything is `core`-only + `heapless`.
//! - **Callback-driven**: Responses are delivered via C function-pointer
//!   callbacks registered at client creation time.

// ── Sub-modules ──────────────────────────────────────────────────────────────

pub mod app;
pub mod callbacks;
pub mod config;
pub mod error;
pub mod models;
pub mod pool;
pub mod serial_client;
pub mod tcp_client;
pub mod transport;

#[cfg(feature = "coils")]
pub mod coils;
#[cfg(feature = "diagnostics")]
pub mod diagnostics;
#[cfg(feature = "discrete-inputs")]
pub mod discrete_inputs;
#[cfg(feature = "fifo")]
pub mod fifo;
#[cfg(feature = "file-record")]
pub mod file_record;
#[cfg(feature = "registers")]
pub mod registers;

// ── Re-exports ───────────────────────────────────────────────────────────────

pub use error::MbusStatusCode;
pub use pool::{MBUS_INVALID_CLIENT_ID, MbusClientId};
