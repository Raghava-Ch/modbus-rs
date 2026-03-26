//! WASM/JS bindings for the modbus-rs client stack.
//!
//! Three layers:
//! 1. `WasmWsTransport`   – implements `Transport` over a browser `WebSocket`.
//! 2. `WasmAppRouter`     – implements the `mbus-client` app traits; resolves/rejects
//!                          JS `Promise`s instead of calling user callbacks directly.
//! 3. `WasmModbusClient`  – `#[wasm_bindgen]` public API; starts a hidden tick loop
//!                          using `spawn_local` + `gloo_timers`.

mod app;
mod net_client;
mod serial_client;

pub use net_client::WasmModbusClient;
pub use serial_client::{WasmSerialModbusClient, WasmSerialPortHandle, request_serial_port};
