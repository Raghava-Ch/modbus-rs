// mbus-ffi: WASM bindings for the modbus-rs stack.
//
// Conditionally compiled: all WASM code lives behind `#[cfg(target_arch = "wasm32")]`
// so native crate builds stay completely unaffected.

#[cfg(target_arch = "wasm32")]
mod wasm;

#[cfg(target_arch = "wasm32")]
pub use wasm::WasmModbusClient;

#[cfg(target_arch = "wasm32")]
pub use wasm::{WasmSerialModbusClient, WasmSerialPortHandle, request_serial_port};

