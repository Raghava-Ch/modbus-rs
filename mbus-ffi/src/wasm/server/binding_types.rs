//! Binding-facing type descriptors for server-side WASM bindings.
//!
//! Note: This module intentionally does not implement transport I/O.
//! Actual WASM transports belong in:
//! - `mbus-network` for websocket/network transport
//! - `mbus-serial` for Web Serial transport

use wasm_bindgen::prelude::*;

/// Transport families planned for browser-side server bindings.
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

/// Lightweight runtime status snapshot for browser-side server observability.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct WasmServerStatusSnapshot {
    transport: WasmServerTransportKind,
    running: bool,
    transport_connected: bool,
    dispatched_requests: u32,
    sent_frames: u32,
    received_frames: u32,
    last_error_present: bool,
}

impl WasmServerStatusSnapshot {
    pub(crate) fn new(
        transport: WasmServerTransportKind,
        running: bool,
        transport_connected: bool,
        dispatched_requests: u32,
        sent_frames: u32,
        received_frames: u32,
        last_error_present: bool,
    ) -> Self {
        Self {
            transport,
            running,
            transport_connected,
            dispatched_requests,
            sent_frames,
            received_frames,
            last_error_present,
        }
    }
}

#[wasm_bindgen]
impl WasmServerStatusSnapshot {
    /// Transport family of this server snapshot.
    pub fn transport(&self) -> WasmServerTransportKind {
        self.transport
    }

    /// Whether server lifecycle is running.
    pub fn running(&self) -> bool {
        self.running
    }

    /// Whether delegated transport reports connected.
    pub fn transport_connected(&self) -> bool {
        self.transport_connected
    }

    /// Number of successful `dispatch_request(...)` calls completed.
    pub fn dispatched_requests(&self) -> u32 {
        self.dispatched_requests
    }

    /// Number of successful `send_frame(...)` calls.
    pub fn sent_frames(&self) -> u32 {
        self.sent_frames
    }

    /// Number of successful `recv_frame(...)` calls.
    pub fn received_frames(&self) -> u32 {
        self.received_frames
    }

    /// Whether a last error message is currently stored.
    pub fn last_error_present(&self) -> bool {
        self.last_error_present
    }
}

/// Configuration for TCP gateway server bindings.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct WasmTcpGatewayConfig {
    ws_url: String,
}

#[wasm_bindgen]
impl WasmTcpGatewayConfig {
    /// Create a new TCP-gateway config from a websocket URL.
    #[wasm_bindgen(constructor)]
    pub fn new(ws_url: &str) -> Self {
        Self {
            ws_url: ws_url.to_string(),
        }
    }

    /// WebSocket endpoint URL.
    pub fn ws_url(&self) -> String {
        self.ws_url.clone()
    }
}

/// Configuration for Web Serial server bindings.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct WasmSerialServerConfig {
    mode: WasmServerTransportKind,
}

#[wasm_bindgen]
impl WasmSerialServerConfig {
    /// Create RTU serial server config.
    pub fn rtu() -> Self {
        Self {
            mode: WasmServerTransportKind::SerialRtu,
        }
    }

    /// Create ASCII serial server config.
    pub fn ascii() -> Self {
        Self {
            mode: WasmServerTransportKind::SerialAscii,
        }
    }

    /// Selected serial mode.
    pub fn mode(&self) -> WasmServerTransportKind {
        self.mode
    }
}

/// High-level design descriptor for phased server binding rollout.
#[wasm_bindgen]
#[derive(Clone, Debug)]
pub struct WasmServerBindingPlan {
    transport: WasmServerTransportKind,
    app_callbacks_required: bool,
    supports_traffic_hooks: bool,
    supports_diagnostics_stats: bool,
    lifecycle_managed: bool,
}

#[wasm_bindgen]
impl WasmServerBindingPlan {
    /// Plan for WebSocket-gateway based server bindings.
    pub fn tcp_gateway() -> Self {
        Self {
            transport: WasmServerTransportKind::TcpGateway,
            app_callbacks_required: true,
            supports_traffic_hooks: true,
            supports_diagnostics_stats: true,
            lifecycle_managed: true,
        }
    }

    /// Plan for Web Serial RTU based server bindings.
    pub fn serial_rtu() -> Self {
        Self {
            transport: WasmServerTransportKind::SerialRtu,
            app_callbacks_required: true,
            supports_traffic_hooks: true,
            supports_diagnostics_stats: true,
            lifecycle_managed: true,
        }
    }

    /// Plan for Web Serial ASCII based server bindings.
    pub fn serial_ascii() -> Self {
        Self {
            transport: WasmServerTransportKind::SerialAscii,
            app_callbacks_required: true,
            supports_traffic_hooks: true,
            supports_diagnostics_stats: true,
            lifecycle_managed: true,
        }
    }

    /// Selected transport family.
    pub fn transport(&self) -> WasmServerTransportKind {
        self.transport
    }

    /// Whether JS app callbacks are required for request dispatch.
    pub fn app_callbacks_required(&self) -> bool {
        self.app_callbacks_required
    }

    /// Whether traffic hook callbacks are part of the plan.
    pub fn supports_traffic_hooks(&self) -> bool {
        self.supports_traffic_hooks
    }

    /// Whether diagnostics counters are part of the plan.
    pub fn supports_diagnostics_stats(&self) -> bool {
        self.supports_diagnostics_stats
    }

    /// Whether lifecycle includes managed start/stop semantics.
    pub fn lifecycle_managed(&self) -> bool {
        self.lifecycle_managed
    }
}
