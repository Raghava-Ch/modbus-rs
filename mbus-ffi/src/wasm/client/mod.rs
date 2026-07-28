//! Client-side WASM/JS bindings for the modbus-rs stack.
//!
//! Four layers:
//! 1. `WasmWsTransport` - implements `Transport` over a browser `WebSocket`.
//! 2. `WasmAppRouter` - implements the `mbus-client` app traits; resolves/rejects
//!    JS `Promise`s instead of calling user callbacks directly.
//! 3. `WasmModbusClient` - `#[wasm_bindgen]` API for WebSocket/TCP gateway usage.
//! 4. `WasmSerialModbusClient` - `#[wasm_bindgen]` API for Web Serial RTU/ASCII usage.
//!
//! Both public client types use the same internal app/router layer so JS-facing
//! response shapes stay consistent across transports.

mod client_serial;
mod client_tcp;
mod command;
pub(crate) mod helpers;
mod response;
mod task;

pub use client_serial::{
    WasmAsciiTransport, WasmRtuTransport, WasmSerialModbusClient, WasmSerialPortHandle,
    request_serial_port,
};
pub use client_tcp::{WasmModbusClient, WasmWsTransport};
