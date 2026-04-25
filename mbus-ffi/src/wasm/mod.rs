//! WASM/JS bindings for the modbus-rs stack.
//!
//! Layout:
//! - `client`: browser-facing client bindings (WebSocket and Web Serial)
//! - `server`: browser-facing server binding design surface

pub mod client;
pub mod server;

pub use client::{WasmModbusClient, WasmSerialModbusClient, WasmSerialPortHandle, request_serial_port};
pub use server::{
	WasmSerialServer, WasmSerialServerConfig, WasmServerBindingPlan, WasmServerStatusSnapshot,
	WasmServerTransportKind, WasmTcpGatewayConfig, WasmTcpServer,
};
