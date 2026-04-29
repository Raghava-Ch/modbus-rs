//! # WebSocket → multiple TCP downstream channels
//!
//! Demonstrates how a single `AsyncWsGatewayServer` can bridge a browser WASM
//! client to **two** different downstream Modbus TCP devices, routing by unit ID.
//!
//! ```text
//! Browser (WASM)                  Gateway (this example)          Downstream
//! ───────────────                 ─────────────────────────       ───────────
//! WasmModbusClient                AsyncWsGatewayServer            Device A
//!   unit 1..=10  ─── WS ────►      channel 0 ──────────────►     (192.168.1.10:502)
//!   unit 11..=20 ─── WS ────►      channel 1 ──────────────►     Device B
//!                                                                 (192.168.1.11:502)
//! ```
//!
//! ## Run
//!
//! ```text
//! cargo run --example ws_to_multi_downstream --features ws-server -p mbus-gateway
//! ```

use std::sync::Arc;

use mbus_gateway::{AsyncWsGatewayServer, RangeRouteTable, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Downstream connections ────────────────────────────────────────────────
    let device_a = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let device_b = TokioTcpTransport::connect("192.168.1.11:502").await?;

    // Indices in this Vec must match the channel index returned by the router.
    let downstreams = vec![
        Arc::new(Mutex::new(device_a)), // channel 0
        Arc::new(Mutex::new(device_b)), // channel 1
    ];

    // ── Routing table ─────────────────────────────────────────────────────────
    // RangeRouteTable maps contiguous unit-ID ranges to channel indices.
    let mut router: RangeRouteTable<4> = RangeRouteTable::new();
    router.add(1, 10, 0).unwrap();  // units  1–10  → channel 0 (device A)
    router.add(11, 20, 1).unwrap(); // units 11–20  → channel 1 (device B)

    // ── Gateway (no extra security for this dev example) ──────────────────────
    let config = WsGatewayConfig::default();

    println!("Listening on ws://0.0.0.0:8502");
    println!("  units  1–10  → 192.168.1.10:502");
    println!("  units 11–20  → 192.168.1.11:502");

    AsyncWsGatewayServer::serve("0.0.0.0:8502", config, router, downstreams).await?;
    Ok(())
}
