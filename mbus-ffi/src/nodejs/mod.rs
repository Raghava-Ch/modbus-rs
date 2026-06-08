//! Node.js (napi-rs) bindings for the Modbus stack.
//!
//! This module provides async, Promise-based bindings to the modbus-rs Rust
//! libraries for Node.js ≥ 20. It exposes:
//!
//! - **TCP transport** — [`crate::nodejs::client_tcp::AsyncTcpTransport`] which owns the TCP connection
//! - **TCP client** — [`crate::nodejs::client_tcp::AsyncTcpModbusClient`] with all standard function codes
//! - **Serial transports** — [`crate::nodejs::client_serial::AsyncRtuTransport`] and [`crate::nodejs::client_serial::AsyncAsciiTransport`] which own the serial port
//! - **Serial client** — [`crate::nodejs::client_serial::AsyncSerialModbusClient`] for RTU and ASCII transports
//! - **TCP server** — [`crate::nodejs::server_tcp::AsyncTcpModbusServer`] with JS callback handlers
//! - **TCP gateway** — [`crate::nodejs::gateway::AsyncTcpGateway`] with declarative unit-ID routing
//!
//! All operations are async and return JS Promises.
//!
//! ## Example
//!
//! ```javascript
//! const { AsyncTcpTransport } = require('modbus-rs');
//!
//! const transport = await AsyncTcpTransport.connect({
//!   host: '127.0.0.1',
//!   port: 502,
//!   timeoutMs: 2000
//! });
//!
//! const client = transport.createClient({ unitId: 1 });
//!
//! const regs = await client.readHoldingRegisters({ address: 0, quantity: 10 });
//! console.log('Registers:', regs);
//!
//! await transport.close();
//! ```

#[cfg(feature = "nodejs-client")]
pub mod client_serial;
#[cfg(feature = "nodejs-client")]
pub mod client_tcp;
pub mod errors;
#[cfg(feature = "nodejs-gateway")]
pub mod gateway;
pub mod runtime;
#[cfg(feature = "nodejs-server")]
pub mod server_serial;
#[cfg(feature = "nodejs-server")]
pub mod server_tcp;

#[cfg(feature = "nodejs-client")]
pub use client_serial::*;
#[cfg(feature = "nodejs-client")]
pub use client_tcp::*;
#[cfg(feature = "nodejs-gateway")]
pub use gateway::*;
#[cfg(feature = "nodejs-server")]
pub use server_serial::*;
#[cfg(feature = "nodejs-server")]
pub use server_tcp::*;
