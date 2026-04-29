//! # WebSocket → TCP gateway — production configuration
//!
//! A hardened gateway suitable for deployment in an environment where the
//! WebSocket port is reachable from the internet (e.g., embedded in an
//! industrial IoT platform or a cloud-hosted bridge).
//!
//! Features demonstrated here:
//!
//! - **Origin allowlist** — only browser connections from `https://hmi.example.com`
//!   are accepted; all other origins receive HTTP `403 Forbidden`.
//! - **Subprotocol enforcement** — the browser must declare
//!   `Sec-WebSocket-Protocol: modbus` or receive HTTP `400 Bad Request`.
//! - **Idle-session timeout** — sessions that send no Modbus requests for
//!   30 seconds are dropped automatically (guards against crashed browser tabs).
//! - **Concurrency cap** — at most 32 simultaneous WASM clients are allowed;
//!   excess connections are rejected at the WS handshake stage.
//! - **Graceful shutdown** — pressing `Ctrl-C` stops accepting new connections;
//!   in-flight sessions complete naturally.
//!
//! ## Run
//!
//! ```text
//! cargo run --example ws_to_tcp_production --features ws-server -p mbus-gateway
//! ```
//!
//! Point a browser WASM client at `ws://localhost:8502` (or whatever address
//! you bind to).  The browser must set `Sec-WebSocket-Protocol: modbus` and
//! its `Origin` must be `https://hmi.example.com`.

use std::sync::Arc;
use std::time::Duration;

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{AsyncWsGatewayServer, UnitRouteTable, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Downstream TCP connections ────────────────────────────────────────────
    //
    // Two Modbus TCP devices are reachable on the local network.
    // Channel 0 → device at 192.168.1.10:502 (units 1–16)
    // Channel 1 → device at 192.168.1.11:502 (units 17–32)
    let ds0 = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let ds1 = TokioTcpTransport::connect("192.168.1.11:502").await?;
    let downstreams = vec![Arc::new(Mutex::new(ds0)), Arc::new(Mutex::new(ds1))];

    // ── Routing table ─────────────────────────────────────────────────────────
    let mut router: UnitRouteTable<32> = UnitRouteTable::new();
    for unit in 1u8..=16 {
        router.add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0).unwrap();
    }
    for unit in 17u8..=32 {
        router.add(UnitIdOrSlaveAddr::new(unit).unwrap(), 1).unwrap();
    }

    // ── Security configuration ────────────────────────────────────────────────
    let config = WsGatewayConfig {
        // Drop any session that is silent for 30 seconds.
        idle_timeout: Some(Duration::from_secs(30)),

        // Reject the 33rd+ concurrent connection at the WS handshake stage.
        max_sessions: 32,

        // Browser must include "modbus" in Sec-WebSocket-Protocol.
        require_modbus_subprotocol: true,

        // Only allow connections from our known HMI origin.
        allowed_origins: vec!["https://hmi.example.com".to_string()],
    };

    // ── Graceful shutdown on Ctrl-C ───────────────────────────────────────────
    let shutdown = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install CTRL+C signal handler");
        println!("\nShutdown signal received — stopping accept loop.");
    };

    println!("WebSocket gateway listening on ws://0.0.0.0:8502");
    println!("Allowed origins:  {:?}", config.allowed_origins);
    println!("Max sessions:     {}", config.max_sessions);
    println!("Idle timeout:     {:?}", config.idle_timeout);

    AsyncWsGatewayServer::serve_with_shutdown(
        "0.0.0.0:8502",
        config,
        router,
        downstreams,
        shutdown,
    )
    .await?;

    Ok(())
}
