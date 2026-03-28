//! WASM/JS bindings for the modbus-rs client stack.
//!
//! Four layers:
//! 1. `WasmWsTransport`   – implements `Transport` over a browser `WebSocket`.
//! 2. `WasmAppRouter`     – implements the `mbus-client` app traits; resolves/rejects
//!                          JS `Promise`s instead of calling user callbacks directly.
//! 3. `WasmModbusClient`  – `#[wasm_bindgen]` API for WebSocket/TCP gateway usage.
//! 4. `WasmSerialModbusClient` – `#[wasm_bindgen]` API for Web Serial RTU/ASCII usage.
//!
//! Both public client types use the same internal app/router layer so JS-facing
//! response shapes stay consistent across transports.

mod app;
mod net_client;
mod serial_client;

pub use net_client::WasmModbusClient;
pub use serial_client::{WasmSerialModbusClient, WasmSerialPortHandle, request_serial_port};
