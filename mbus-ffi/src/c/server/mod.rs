//! Native C FFI bindings for the Modbus server stack.
//!
//! This module provides a complete C API for creating and operating Modbus TCP and Serial
//! (RTU/ASCII) servers. The server application logic is supplied by C code through a
//! table of function-pointer callbacks ([`MbusServerHandlers`]).
//!
//! # Design
//!
//! - **Per-FC-group typed callbacks**: C code receives fully-parsed request fields and
//!   fills in a response struct. The Rust layer handles all PDU framing — C never touches
//!   raw Modbus bytes.
//! - **None handler slots**: If a callback slot is `NULL`, the server automatically
//!   responds with `ExceptionCode::IllegalFunction` — no C code required for unsupported FCs.
//! - **ID-based API**: C code receives an opaque `MbusServerId` (u16). High byte encodes
//!   the pool type (0x10 = TCP server, 0x11 = Serial server); low byte is the slot index.
//!   `0xFFFF` is the reserved invalid sentinel.
//! - **Transport-agnostic**: The C caller supplies `MbusTransportCallbacks` to manage the
//!   underlying socket or serial port. The same callback struct used by the client is reused.
//! - **External locking**: Callers must serialise `mbus_tcp_server_poll` and all mutating
//!   calls (connect/disconnect) using `mbus_server_lock(id)` / `mbus_server_unlock(id)`.
//!   Pool allocation/free must be serialised with `mbus_server_pool_lock/unlock`.

pub mod app;
pub mod callbacks;
pub mod config;
pub mod pool;
pub mod serial_server;
pub mod tcp_server;

pub use callbacks::{MbusServerExceptionCode, MbusServerHandlers};
pub use pool::{MBUS_INVALID_SERVER_ID, MbusServerId};
