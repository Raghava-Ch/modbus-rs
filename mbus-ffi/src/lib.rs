//! WASM bindings for the Modbus workspace.
//!
//! This crate is intentionally wasm-focused. Public bindings are exported only
//! on `wasm32` targets; native builds keep this crate inert.
//!
//! Browser-facing APIs:
//! - `WasmModbusClient` for WebSocket/TCP gateway usage
//! - `WasmSerialModbusClient` for Web Serial (RTU/ASCII)
//! - `request_serial_port` and `WasmSerialPortHandle` helpers

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmModbusClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::{WasmSerialModbusClient, WasmSerialPortHandle, request_serial_port};
