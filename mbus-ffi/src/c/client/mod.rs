//! Native C FFI bindings for the Modbus client stack.
//!
//! This module provides a complete, `no_std`-compatible C API for creating
//! and operating Modbus TCP and Serial (RTU/ASCII) clients.
//!
//! # Design
//!
//! - **Split typed static pools**: TCP clients occupy `tcp_slots[0..MAX_TCP_CLIENTS]`,
//!   Serial RTU clients occupy `serial_rtu_slots[0..MAX_SERIAL_CLIENTS]`, and
//!   Serial ASCII clients occupy `serial_ascii_slots[0..MAX_SERIAL_CLIENTS]`.
//!   Pool sizes are configured via `MBUS_MAX_TCP_CLIENTS` and `MBUS_MAX_SERIAL_CLIENTS`
//!   environment variables at build time (both default to 1).
//! - **ID-based API**: C code receives an opaque `MbusClientId` (u16). The high
//!   byte encodes the pool type (0x00 = TCP, 0x01 = Serial RTU, 0x02 = Serial ASCII);
//!   the low byte is the slot index. `0xFFFF` is the reserved invalid sentinel.
//! - **Zero heap allocation**: Everything is `core`-only + `heapless`.
//! - **Callback-driven**: Responses are delivered via C function-pointer
//!   callbacks registered at client creation time.

// ── Sub-modules ──────────────────────────────────────────────────────────────

pub mod app;
pub mod callbacks;
pub mod config;
pub mod models;
pub mod pool;
pub mod serial_client;
pub mod tcp_client;

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

pub use crate::c::error::MbusStatusCode;
pub use pool::{MBUS_INVALID_CLIENT_ID, MbusClientId};
