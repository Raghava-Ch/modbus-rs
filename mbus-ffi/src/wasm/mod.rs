//! WASM/JS bindings for the modbus-rs stack.
//!
//! Layout:
//! - `client`: browser-facing client bindings (WebSocket and Web Serial)
//! - `server`: browser-facing server binding design surface

#[cfg(feature = "wasm-client")]
pub mod client;
#[cfg(feature = "wasm-client")]
pub mod error_codes;
#[cfg(feature = "wasm-server")]
pub mod server;

#[cfg(target_arch = "wasm32")]
mod wasm_types;

#[cfg(feature = "wasm-client")]
pub use client::{
    WasmModbusClient, WasmSerialModbusClient, WasmSerialPortHandle, WasmRtuTransport, WasmAsciiTransport,
    WasmWsTransport, request_serial_port,
};

#[cfg(feature = "wasm-client")]
pub use error_codes::{ModbusErrorCode, get_modbus_error_code};

#[cfg(feature = "wasm-server")]
pub use server::{
    WasmSerialServer, WasmSerialServerOptions, WasmServerTransportKind, WasmTcpServer,
    WasmTcpServerOptions,
};
