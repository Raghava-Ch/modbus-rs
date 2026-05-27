# Modbus Gateway — Quick Start

This page walks you through the three ways to run a Modbus gateway:
**sync (no_std compatible)**, **async TCP (Tokio)**, and **async WebSocket (Tokio, for WASM clients)**.

## Prerequisites

```toml
[dependencies]
modbus-rs = { version = "0.12.0", features = ["gateway", "network-tcp", "serial-rtu"] }
```

## Sync: TCP upstream → RTU downstream
<!-- validate: no_run -->
```rust,ignore
use std::net::TcpListener;
use std::time::Duration;
use std::{env, thread::sleep};

use modbus_rs::gateway::{
    DownstreamChannel, GatewayServices, NoopEventHandler, PollOutcome, UnitRouteTable,
};
use modbus_rs::{
    UnitIdOrSlaveAddr, BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, StdRtuTransport, StdTcpServerTransport, Transport,
};

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bind_addr = env::var("MBUS_GATEWAY_BIND").unwrap_or_else(|_| "127.0.0.1:5502".into());
    let serial_port =
        env::var("MBUS_GATEWAY_SERIAL").unwrap_or_else(|_| "/dev/cu.usbserial-A1010CA6".into());

    // ── Upstream: listen for TCP connections ──────────────────────────────────
    println!("Binding upstream TCP on {bind_addr}");
    let listener = TcpListener::bind(&bind_addr)?;
    println!("Waiting for upstream TCP connection on {bind_addr}");
    let (stream, peer) = listener.accept()?;
    println!("Accepted upstream TCP connection from {peer}");
    let upstream = StdTcpServerTransport::new(stream);

    // ── Downstream: connect to RTU slave ─────────────────────────────────────
    println!("Opening serial downstream on {serial_port}");
    let serial_config = ModbusSerialConfig {
        port_path: serial_port
            .as_str()
            .try_into()
            .map_err(|_| anyhow::anyhow!("serial port path too long"))?,
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Custom(115200),
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 500,
        retry_attempts: 0,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };

    let mut downstream_transport = StdRtuTransport::new();
    downstream_transport.connect(&ModbusConfig::Serial(serial_config))?;
    println!("Serial downstream ready");

    // ── Routing table ─────────────────────────────────────────────────────────
    // Route all units 1–32 to channel 0 (the single RTU bus).
    let mut router: UnitRouteTable<32> = UnitRouteTable::new();
    for unit_id in 1u8..=32 {
        if let Ok(uid) = UnitIdOrSlaveAddr::new(unit_id) {
            router.add(uid, 0).ok();
        }
    }

    // ── Gateway ───────────────────────────────────────────────────────────────
    let mut gateway: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _> =
        GatewayServices::new(router, NoopEventHandler, 500);

    gateway.add_upstream(upstream)?;
    gateway.add_downstream(DownstreamChannel::new(downstream_transport))?;

    println!("Gateway running — forwarding TCP → RTU");

    let mut shutdown = false;
    loop {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        match gateway.poll(now_ms) {
            PollOutcome::AllUpstreamsDisconnected => {
                println!("All upstreams disconnected; shutting down");
                shutdown = true;
            }
            _ => {}
        }
        if shutdown {
            break;
        }
        // Yield briefly to avoid hogging CPU in example loop
        sleep(Duration::from_millis(1));
    }
    Ok(())
}
```

## Async WebSocket: WASM upstream → TCP downstream

Add the `upstream-ws` feature:

```toml
[dependencies]
mbus-gateway = { version = "0.12.0", features = ["upstream-ws"] }
```

<!-- validate: skip -->
```rust,ignore
use std::sync::Arc;
use std::time::Duration;

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{AsyncWsGatewayServer, NoopEventHandler, UnitRouteTable, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // ── Downstream TCP connections ────────────────────────────────────────────
    //
    // Two Modbus TCP devices are reachable on the local network.
    // Channel 0 → device at 192.168.1.10:502 (units 1–16)
    // Channel 1 → device at 192.168.1.11:502 (units 17–32)
    let ds0 = TokioTcpTransport::connect("192.168.1.10:502")
        .await
        .map_err(|e| format!("{:?}", e))?;
    let ds1 = TokioTcpTransport::connect("192.168.1.11:502")
        .await
        .map_err(|e| format!("{:?}", e))?;
    let downstreams = vec![Arc::new(Mutex::new(ds0)), Arc::new(Mutex::new(ds1))];

    // ── Routing table ─────────────────────────────────────────────────────────
    let mut router: UnitRouteTable<32> = UnitRouteTable::new();
    for unit in 1u8..=16 {
        router
            .add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0)
            .unwrap();
    }
    for unit in 17u8..=32 {
        router
            .add(UnitIdOrSlaveAddr::new(unit).unwrap(), 1)
            .unwrap();
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

    let handler = Arc::new(Mutex::new(NoopEventHandler));
    AsyncWsGatewayServer::serve_with_shutdown(
        "0.0.0.0:8502",
        config,
        router,
        downstreams,
        handler,
        Duration::from_secs(1),
        shutdown,
    )
    .await?;

    Ok(())
}
```

The browser-side `WasmModbusClient` requires **no code changes** — it just
points its WebSocket URL at `ws://<gateway-host>:8502` instead of connecting
directly to the device.

See [ws_gateway.md](ws_gateway.md) for the full WebSocket gateway reference.

## Runnable Examples

Two end-to-end gateway examples are available in the [modbus-rs](../../modbus-rs/examples/gateway/) crate:

### Sync: TCP upstream ↔ RTU downstream (`modbus_rs_gateway_sync_tcp_to_rtu`)

A poll-driven gateway with no async runtime. Accepts Modbus TCP requests on a listening port and
forwards them to an RTU slave on a serial bus. Demonstrates environment-variable configuration.

```sh
MBUS_GATEWAY_BIND=0.0.0.0:5502 \
MBUS_GATEWAY_SERIAL=/dev/ttyUSB0 \
  cargo run --example modbus_rs_gateway_sync_tcp_to_rtu \
    --features gateway,serial-rtu,network-tcp
```

**Source:** [modbus-rs/examples/gateway/sync_tcp_to_rtu.rs](../../modbus-rs/examples/gateway/sync_tcp_to_rtu.rs)

### Async: TCP upstream ↔ TCP downstream with unit-ID rewriting (`modbus_rs_gateway_async_tcp_to_tcp`)

An async (Tokio) gateway that bridges a TCP upstream master to a TCP downstream server.
Demonstrates the `UnitIdRewriteRouter` to remap unit IDs by a fixed offset (upstream unit 1 → downstream unit 101).

```sh
MBUS_GATEWAY_UPSTREAM=0.0.0.0:5502 \
MBUS_GATEWAY_DOWNSTREAM=192.168.1.10:502 \
  cargo run --example modbus_rs_gateway_async_tcp_to_tcp \
    --features gateway,network-tcp,async
```

**Source:** [modbus-rs/examples/gateway/async_tcp_to_tcp.rs](../../modbus-rs/examples/gateway/async_tcp_to_tcp.rs)
