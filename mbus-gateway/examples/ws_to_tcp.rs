//! WebSocket → raw TCP gateway example.
//!
//! Listens on `0.0.0.0:8502` for browser WebSocket connections and forwards
//! each Modbus request to a downstream device at `127.0.0.1:502` via raw TCP.
//!
//! Run with:
//! ```text
//! cargo run --example ws_to_tcp --features ws-server -p mbus-gateway
//! ```
//!
//! The browser-side `WasmModbusClient` (mbus-ffi) can then connect to
//! `ws://localhost:8502` and communicate with the downstream device.

use std::sync::Arc;
use std::time::Duration;

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{AsyncWsGatewayServer, UnitRouteTable, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Routing table ─────────────────────────────────────────────────────────
    // Forward unit 1..=16 to channel 0 (the single downstream TCP connection).
    let mut router: UnitRouteTable<16> = UnitRouteTable::new();
    for unit in 1u8..=16 {
        router
            .add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0)
            .unwrap();
    }

    // ── Downstream TCP connection ─────────────────────────────────────────────
    let downstream = TokioTcpTransport::connect("127.0.0.1:502").await?;
    let shared = Arc::new(Mutex::new(downstream));

    // ── Gateway configuration ─────────────────────────────────────────────────
    let config = WsGatewayConfig {
        // Drop idle browser sessions after 30 seconds of inactivity.
        idle_timeout: Some(Duration::from_secs(30)),
        // Allow at most 64 concurrent WASM clients.
        max_sessions: 64,
        // Require browsers to declare the "modbus" WebSocket subprotocol.
        require_modbus_subprotocol: true,
        // Allow all origins in this example (restrict in production).
        allowed_origins: Vec::new(),
    };

    println!("WebSocket gateway listening on ws://0.0.0.0:8502");
    println!("Forwarding to downstream Modbus TCP at 127.0.0.1:502");

    AsyncWsGatewayServer::serve("0.0.0.0:8502", config, router, vec![shared]).await?;

    Ok(())
}
