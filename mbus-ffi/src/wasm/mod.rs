//! WASM/JS bindings for the modbus-rs stack.
//!
//! Layout:
//! - `client`: browser-facing client bindings (WebSocket and Web Serial)
//! - `server`: browser-facing server binding design surface

#[cfg(feature = "wasm-client")]
pub mod client;
#[cfg(feature = "wasm-server")]
pub mod server;

#[cfg(feature = "wasm-client")]
pub use client::{
    WasmModbusClient, WasmSerialModbusClient, WasmSerialPortHandle, WasmSerialTransport,
    WasmTcpTransport, request_serial_port,
};

#[cfg(feature = "wasm-server")]
pub use server::{
    WasmSerialServer, WasmSerialServerOptions, WasmServerTransportKind, WasmTcpServer,
    WasmTcpServerOptions,
};
