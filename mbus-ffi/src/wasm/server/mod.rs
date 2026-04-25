//! Server-side WASM binding implementation (phase 1).
//!
//! Exposes lifecycle + JS callback dispatch for:
//! - TCP gateway server endpoints
//! - Web Serial server endpoints
//!
//! Transport implementations are owned by transport crates only:
//! - `mbus-network` (WASM network transport)
//! - `mbus-serial` (WASM serial transport)
//!
//! This module owns only JS binding types, lifecycle facade, and callback bridge.

mod adapters;
mod bridge;
mod binding_types;
mod serial_server;
mod tcp_server;

pub use serial_server::WasmSerialServer;
pub use tcp_server::WasmTcpServer;
pub use binding_types::{
    WasmSerialServerConfig, WasmServerBindingPlan, WasmServerStatusSnapshot,
    WasmServerTransportKind, WasmTcpGatewayConfig,
};
