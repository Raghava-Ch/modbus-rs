# Modbus Gateway — Quick Start

This page walks you through the three ways to run a Modbus gateway:
**sync (no_std compatible)**, **async TCP (Tokio)**, and **async WebSocket (Tokio, for WASM clients)**.

## Prerequisites

```toml
[dependencies]
mbus-gateway = "0.8.0"
modbus-rs = { version = "0.8.0", features = ["gateway", "network-tcp", "serial-rtu"] }
```

## Sync: TCP upstream → RTU downstream

```rust,ignore
use modbus_rs::{ModbusTcpConfig, ModbusSerialConfig, StdTcpServerTransport, StdRtuTransport,
                BaudRate, DataBits, Parity, SerialMode};
use mbus_gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;

// 1. Upstream TCP transport (accepts incoming connections)
let tcp_config = ModbusTcpConfig {
    host: "0.0.0.0".into(),
    port: 502,
    response_timeout_ms: 1000,
    connection_timeout_ms: 5000,
};
let mut upstream = StdTcpServerTransport::new();
upstream.connect(&modbus_rs::ModbusConfig::Tcp(tcp_config)).unwrap();

// 2. Downstream RTU transport
let serial_config = ModbusSerialConfig {
    port: "/dev/ttyUSB0".into(),
    baud_rate: BaudRate::Baud9600,
    data_bits: DataBits::Eight,
    parity: Parity::None,
    stop_bits: modbus_rs::transport::StopBits::One,
    response_timeout_ms: 500,
    mode: SerialMode::Rtu,
};
let mut downstream = StdRtuTransport::new();
downstream.connect(&modbus_rs::ModbusConfig::Serial(serial_config)).unwrap();

// 3. Routing: units 1–10 → channel 0
let mut router: UnitRouteTable<10> = UnitRouteTable::new();
for id in 1u8..=10 {
    router.add(UnitIdOrSlaveAddr::new(id).unwrap(), 0).unwrap();
}

// 4. Create and run gateway
let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _, 1> =
    GatewayServices::new(upstream, router, NoopEventHandler);
gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
gw.set_max_downstream_recv_attempts(1); // blocking serial recv

loop {
    let _ = gw.poll();
}
```

## Async: TCP upstream → TCP downstream

<!-- validate: skip -->
```rust,ignore
use std::sync::Arc;
use tokio::sync::Mutex;
use mbus_gateway::{AsyncTcpGatewayServer, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to downstream server
    let ds = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(ds));

    // Build route table
    let mut router: UnitRouteTable<10> = UnitRouteTable::new();
    for id in 1u8..=10 {
        router.add(UnitIdOrSlaveAddr::new(id).unwrap(), 0).unwrap();
    }

    // Run the gateway (infinite loop, returns Infallible or I/O error)
    AsyncTcpGatewayServer::serve("0.0.0.0:502", router, vec![shared]).await?;
    Ok(())
}
```

## Async WebSocket: WASM upstream → TCP downstream

Add the `ws-server` feature:

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", features = ["ws-server"] }
mbus-network = { version = "0.8.0", features = ["async"] }
```

<!-- validate: skip -->
```rust,ignore
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use mbus_gateway::{AsyncWsGatewayServer, UnitRouteTable, WsGatewayConfig};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to downstream Modbus TCP device
    let ds = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(ds));

    // Build route table
    let mut router: UnitRouteTable<10> = UnitRouteTable::new();
    for id in 1u8..=10 {
        router.add(UnitIdOrSlaveAddr::new(id).unwrap(), 0).unwrap();
    }

    // Configure the WebSocket gateway
    let config = WsGatewayConfig {
        idle_timeout: Some(Duration::from_secs(30)),
        max_sessions: 32,
        require_modbus_subprotocol: true,
        allowed_origins: vec!["https://hmi.example.com".to_string()],
    };

    // Listen for browser WebSocket connections on port 8502
    // The browser WasmModbusClient connects to ws://localhost:8502
    AsyncWsGatewayServer::serve("0.0.0.0:8502", config, router, vec![shared]).await?;
    Ok(())
}
```

The browser-side `WasmModbusClient` requires **no code changes** — it just
points its WebSocket URL at `ws://<gateway-host>:8502` instead of connecting
directly to the device.

See [ws_gateway.md](ws_gateway.md) for the full WebSocket gateway reference.

## Runnable Examples

Two end-to-end gateway examples are available in the [modbus-rs](../modbus-rs/examples/gateway/) crate:

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
