//! Binding-facing type descriptors for server-side WASM bindings.

use wasm_bindgen::prelude::*;

/// Transport families for browser-side server bindings.
#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmServerTransportKind {
    /// Server loop receives requests via a WebSocket gateway.
    TcpGateway,
    /// Server loop receives RTU frames via Web Serial.
    SerialRtu,
    /// Server loop receives ASCII frames via Web Serial.
    SerialAscii,
}

#[wasm_bindgen]
extern "C" {
    /// Options for binding a WASM TCP Modbus server.
    #[wasm_bindgen(typescript_type = "WasmTcpServerOptions")]
    pub type WasmTcpServerOptions;

    /// Options for binding a WASM Serial Modbus server.
    #[wasm_bindgen(typescript_type = "WasmSerialServerOptions")]
    pub type WasmSerialServerOptions;
}
