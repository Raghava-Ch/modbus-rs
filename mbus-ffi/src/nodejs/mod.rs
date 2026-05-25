//! Node.js (napi-rs) bindings for the Modbus stack.
//!
//! This module provides async, Promise-based bindings to the modbus-rs Rust
//! libraries for Node.js ≥ 20. It exposes:
//!
//! - **TCP client** — [`crate::nodejs::client_tcp::AsyncTcpModbusClient`] with all standard function codes
//! - **Serial client** — [`crate::nodejs::client_serial::AsyncSerialModbusClient`] for RTU and ASCII transports
//! - **TCP server** — [`crate::nodejs::server_tcp::AsyncTcpModbusServer`] with JS callback handlers
//! - **TCP gateway** — [`crate::nodejs::gateway::AsyncTcpGateway`] with declarative unit-ID routing
//!
//! All operations are async and return JS Promises.
//!
//! ## Example
//!
//! ```javascript
//! const { AsyncTcpModbusClient } = require('modbus-rs');
//!
//! const client = await AsyncTcpModbusClient.connect({
//!   host: '127.0.0.1',
//!   port: 502,
//!   unitId: 1,
//!   timeoutMs: 2000
//! });
//!
//! const regs = await client.readHoldingRegisters({ address: 0, quantity: 10 });
//! console.log('Registers:', regs);
//!
//! await client.close();
//! ```

pub mod client_serial;
pub mod client_tcp;
pub mod errors;
pub mod gateway;
pub mod runtime;
pub mod server_serial;
pub mod server_tcp;

pub use client_serial::*;
pub use client_tcp::*;
pub use gateway::*;
pub use server_serial::*;
pub use server_tcp::*;
