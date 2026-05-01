//! Node.js (napi-rs) bindings for the Modbus stack.
//!
//! This module provides async, Promise-based bindings to the modbus-rs Rust
//! libraries for Node.js ≥ 20. It exposes:
//!
//! - **TCP client** — [`AsyncTcpModbusClient`] with all standard function codes
//! - **Serial client** — [`AsyncSerialModbusClient`] for RTU and ASCII transports
//! - **TCP server** — [`AsyncTcpModbusServer`] with JS callback handlers
//! - **TCP gateway** — [`AsyncTcpGateway`] with declarative unit-ID routing
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

pub mod runtime;
pub mod errors;
pub mod client_tcp;
pub mod client_serial;
pub mod server_tcp;
pub mod gateway;

pub use client_tcp::*;
pub use client_serial::*;
pub use server_tcp::*;
pub use gateway::*;
