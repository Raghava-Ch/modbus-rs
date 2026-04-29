# mbus-gateway

A Modbus gateway runtime that bridges two Modbus networks.

The gateway acts as a **server** to upstream clients (e.g., SCADA over TCP) and as a **client** to downstream devices (e.g., RTU slaves on a serial bus). It accepts upstream requests, routes them by unit ID to the correct downstream channel, translates ADU framing (TCP MBAP ↔ RTU CRC ↔ ASCII LRC), forwards the PDU, and returns the response.

## Feature Flags

| Feature | Default | Description |
|---------|---------|-------------|
| `async` | ✓ | Async Tokio gateway (`AsyncTcpGatewayServer`) |
| `ws-server` | ✗ | WebSocket gateway (`AsyncWsGatewayServer`) for WASM clients |
| `logging` | ✓ | `log` facade integration |
| `traffic` | ✗ | Raw TX/RX frame callbacks in `GatewayEventHandler` |

## Quick Start — Sync Gateway (no_std-compatible)

```rust,no_run
use mbus_gateway::{
    DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable,
};
use mbus_core::transport::UnitIdOrSlaveAddr;

// Build a routing table: unit 1 → channel 0, unit 2 → channel 1
let mut router: UnitRouteTable<8> = UnitRouteTable::new();
router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
router.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap();

// Create the gateway (upstream + routing policy + event handler)
// let mut gw: GatewayServices<MyUpstreamTransport, MyDownstreamTransport, _, _, 2> =
//     GatewayServices::new(upstream_transport, router, NoopEventHandler);

// Register downstream channels (index matches routing policy return values)
// gw.add_downstream(DownstreamChannel::new(downstream_0)).unwrap();
// gw.add_downstream(DownstreamChannel::new(downstream_1)).unwrap();

// Poll-driven loop
// loop {
//     match gw.poll() {
//         Ok(()) => {}
//         Err(e) => eprintln!("gateway error: {:?}", e),
//     }
// }
```

## Quick Start — Async Gateway (Tokio)

```rust,no_run
use std::sync::Arc;
use tokio::sync::Mutex;
use mbus_gateway::{AsyncTcpGatewayServer, PassthroughRouter};
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Single downstream TCP server
    let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(downstream));

    // Route all traffic to channel 0 (passthrough)
    AsyncTcpGatewayServer::serve(
        "0.0.0.0:502",
        PassthroughRouter,
        vec![shared],
    )
    .await?;
    Ok(())
}
```

## Routing Policies

### `UnitRouteTable<N>` — Exact unit-ID mapping

```rust
use mbus_gateway::UnitRouteTable;
use mbus_core::transport::UnitIdOrSlaveAddr;

let mut table: UnitRouteTable<8> = UnitRouteTable::new();
table.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap(); // unit 1 → channel 0
table.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap(); // unit 2 → channel 1
```

### `RangeRouteTable<N>` — Range-based routing

```rust
use mbus_gateway::RangeRouteTable;

let mut table: RangeRouteTable<4> = RangeRouteTable::new();
table.add(1, 32, 0).unwrap();   // units 1–32  → channel 0
table.add(33, 64, 1).unwrap();  // units 33–64 → channel 1
```

### `PassthroughRouter` — Forward everything to channel 0

```rust
use mbus_gateway::PassthroughRouter;
let router = PassthroughRouter;
```

### `UnitIdRewriteRouter<R>` — Add an offset to the downstream unit ID

```rust
use mbus_gateway::{PassthroughRouter, UnitIdRewriteRouter};

// Upstream unit 5 → downstream unit 105 (offset +100)
let router = UnitIdRewriteRouter::new(PassthroughRouter, 100);
```

### Custom Policy

Implement `GatewayRoutingPolicy`:

```rust
use mbus_gateway::GatewayRoutingPolicy;
use mbus_core::transport::UnitIdOrSlaveAddr;

struct MyRouter;

impl GatewayRoutingPolicy for MyRouter {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        match unit.get() {
            1..=50  => Some(0), // channel 0
            51..=100 => Some(1), // channel 1
            _ => None,
        }
    }
}
```

## Observability

Implement `GatewayEventHandler` to receive lifecycle events:

```rust
use mbus_gateway::GatewayEventHandler;
use mbus_core::transport::UnitIdOrSlaveAddr;

struct MyHandler { requests: u32 }

impl GatewayEventHandler for MyHandler {
    fn on_forward(&mut self, _session_id: u8, unit: UnitIdOrSlaveAddr, channel_idx: usize) {
        self.requests += 1;
        println!("Forwarding unit={} to channel={}", unit.get(), channel_idx);
    }
    fn on_routing_miss(&mut self, _session_id: u8, unit: UnitIdOrSlaveAddr) {
        eprintln!("No route for unit={}", unit.get());
    }
    fn on_downstream_timeout(&mut self, _session_id: u8, _internal_txn: u16) {
        eprintln!("Downstream device did not respond");
    }
}
```

## no_std Usage

Disable the default features and enable only what you need:

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", default-features = false }
```

All routing, transaction-ID mapping, and session management use `heapless::Vec`/`Deque` with const-generic capacities. No allocator is required for the sync gateway core.

## Architecture

```
Upstream (TCP/Serial)
        │
        ▼
  ┌─────────────────────────────────┐
  │        GatewayServices          │
  │   ┌──────────┐  ┌──────────┐   │
  │   │ TxnMap   │  │  Router  │   │
  │   └──────────┘  └──────────┘   │
  └───────────────┬─────────────────┘
                  │  (by channel index)
        ┌─────────┴──────────┐
        ▼                    ▼
  Channel 0              Channel 1
  (RTU Bus A)           (RTU Bus B)
```

See `documentation/gateway/` for detailed architecture and usage documentation.

## WebSocket Gateway (WASM client → raw TCP/serial)

Enable the `ws-server` feature to add `AsyncWsGatewayServer`, which accepts
WebSocket connections from browser-side WASM clients and forwards each Modbus
request to any `AsyncTransport` downstream (TCP, RTU, ASCII):

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", features = ["ws-server"] }
```

```rust,no_run
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use mbus_gateway::{AsyncWsGatewayServer, PassthroughRouter, WsGatewayConfig};
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(downstream));

    let config = WsGatewayConfig {
        idle_timeout: Some(Duration::from_secs(30)),
        max_sessions: 32,
        require_modbus_subprotocol: true,
        allowed_origins: vec!["https://hmi.example.com".to_string()],
    };

    // Browser WasmModbusClient connects to ws://localhost:8502
    AsyncWsGatewayServer::serve("0.0.0.0:8502", config, PassthroughRouter, vec![shared]).await?;
    Ok(())
}
```

`WsGatewayConfig` options:

| Field | Default | Description |
|-------|---------|-------------|
| `idle_timeout` | `None` | Drop sessions silent for this long (zombie tab guard) |
| `max_sessions` | `0` = unlimited | Reject excess connections at the handshake stage |
| `require_modbus_subprotocol` | `false` | Enforce `Sec-WebSocket-Protocol: modbus` header |
| `allowed_origins` | `[]` = allow all | CORS origin allowlist |

See `documentation/gateway/ws_gateway.md` for the full reference including
multi-downstream routing, RTU serial downstream, and graceful shutdown.

