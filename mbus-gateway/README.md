# mbus-gateway

A Modbus gateway runtime that bridges two Modbus networks.

The gateway acts as a **server** to upstream clients (e.g., SCADA over TCP) and as a **client** to downstream devices (e.g., RTU slaves on a serial bus). It accepts upstream requests, routes them by unit ID to the correct downstream channel, translates ADU framing (TCP MBAP ↔ RTU CRC ↔ ASCII LRC), forwards the PDU, and returns the response.

### Core Architecture & Design Highlights
- **Fully Non-Blocking Sync Core**: Built on a deterministic, event-driven per-channel state machine. Driving the gateway via `poll(now_ms)` executes exactly one non-blocking call per transport per cycle—eliminating CPU starvation and spin-wait busy loops.
- **Pure `no_std` / Bare-Metal Compliance**: Operates completely without `alloc` or standard library dependencies. Zero dynamic heap allocation and zero `dyn` trait objects. All components (receive buffers, routing tables, transaction maps, and queues) are bounded at compile time via const generics and backed by `heapless`.
- **Multi-Session & Heterogeneous Transports**: Supports up to `N_UPSTREAM` concurrent sessions and `N_DOWNSTREAM` concurrent channels. The zero-cost `GatewayUpstream` enum wraps any combination of TCP and serial upstreams without dynamic dispatch overhead.
- **Transient FIFO Request Queueing**: A configurable `N_PENDING` queue buffers incoming requests when downstream channels are busy, preventing head-of-line blocking and eliminating dropped frames during short transient bursts.

## Feature Flags

The gateway is highly modular. Disable default features for a minimal, `no_std` bare-metal footprint, or enable specific transports as needed:

| Feature | Default | Description |
|---------|---------|-------------|
| **Core Features** | | |
| `async` | ✓ | Enables the asynchronous Tokio-backed gateway runtime. |
| `logging` | ✓ | Integrates the standard `log` facade for diagnostic messages. |
| `traffic` | ✗ | Enables raw TX/RX frame callbacks in `GatewayEventHandler` for traffic sniffing. |
| **Upstream Transports** | | |
| `upstream-tcp` | ✓ | Modbus TCP server-side listener (implies `async`). |
| `upstream-ws` | ✓ | WebSocket gateway (`AsyncWsGatewayServer`) for WASM browser-side clients (implies `async`). |
| `upstream-serial-rtu` | ✓ | Modbus RTU serial upstream interface. |
| `upstream-serial-ascii`| ✓ | Modbus ASCII serial upstream interface. |
| **Downstream Transports** | | |
| `downstream-tcp` | ✓ | Sync and async Modbus TCP client downstream. |
| `downstream-serial-rtu` | ✓ | Sync and async Modbus RTU serial client downstream. |
| `downstream-serial-ascii`| ✓ | Sync and async Modbus ASCII serial client downstream. |

## FFI Bindings

C and Python bindings for the gateway live in the `mbus-ffi` crate:

* **C / `no_std`** — enable the `c-gateway` feature on `mbus-ffi`. The
  `cbindgen` build script writes the C header to
  `target/mbus-ffi/include/modbus_rs_gateway.h`. A runnable demo lives at
  `mbus-ffi/examples/c_gateway_demo/`.
* **Python** — enable the `python-gateway` feature on `mbus-ffi` (build with
  `maturin develop --features python,python-gateway,full`). The bindings
  expose `modbus_rs.TcpGateway` (sync) and `modbus_rs.AsyncTcpGateway`
  (asyncio); demos live in `mbus-ffi/examples/python_gateway/`.

## Quick Start — Sync Gateway (no_std-compatible)

```rust
use mbus_gateway::{
    DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable, PollOutcome,
};
use mbus_core::transport::UnitIdOrSlaveAddr;

// 1. Build a routing table: unit 1 → channel 0, unit 2 → channel 1
let mut router: UnitRouteTable<8> = UnitRouteTable::new();
router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
router.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap();

// 2. Create the gateway (router, event handler, downstream timeout in ms)
// By default, holds 1 upstream, 1 downstream, 4 max in-flight txns, 0 pending queue items
let mut gw: GatewayServices<MyUpstream, MyDownstream, _, _> =
    GatewayServices::new(router, NoopEventHandler, 500 /* 500ms timeout */);

// 3. Register the upstream and downstream channels
gw.add_upstream(upstream_transport).unwrap();
gw.add_downstream(DownstreamChannel::new(downstream_0)).unwrap();
gw.add_downstream(DownstreamChannel::new(downstream_1)).unwrap();

// 4. Tight non-blocking poll-driven loop (pass absolute system clock milliseconds)
loop {
    let now_ms = get_monotonic_time_ms();
    match gw.poll(now_ms) {
        PollOutcome::Active => {
            // At least one packet was forwarded or completed this cycle
        }
        PollOutcome::Idle => {
            // No events to process
        }
        PollOutcome::AllUpstreamsDisconnected => {
            // Teardown the session
            break;
        }
    }
}
```

## Quick Start — Async Gateway (Tokio)

<!-- validate: skip -->
```rust,ignore
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
mbus-gateway = { version = "0.12.0", default-features = false }
```

All routing, transaction-ID mapping, and session management use `heapless::Vec`/`Deque` with const-generic capacities. No allocator is required for the sync gateway core.

## Architecture

```
 Upstream 0 (TCP)       Upstream 1 (Serial)
         │                       │
         ▼                       ▼
  ┌───────────────────────────────────────────┐
  │              GatewayServices              │
  │  ┌──────────┐ ┌──────────┐ ┌───────────┐  │
  │  │  TxnMap  │ │  Router  │ │  FIFO Q   │  │
  │  └──────────┘ └──────────┘ └───────────┘  │
  └───────────────────┬───────────────────────┘
                      │ (by channel index)
            ┌─────────┴──────────┐
            ▼                    ▼
      Channel 0              Channel 1
     (RTU Bus A)            (RTU Bus B)
```

See `documentation/gateway/` for detailed architecture and usage documentation.

## WebSocket Gateway (WASM client → raw TCP/serial)

Enable the `upstream-ws` feature to add `AsyncWsGatewayServer`, which accepts
WebSocket connections from browser-side WASM clients and forwards each Modbus
request to any `AsyncTransport` downstream (TCP, RTU, ASCII):

```toml
[dependencies]
mbus-gateway = { version = "0.12.0", features = ["upstream-ws"] }
```

<!-- validate: verify -->
```rust,ignore
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


## Security Hardening Checklist (production rollout)

Modbus/TCP and Modbus over WebSocket carry no authentication, no integrity
protection, and no confidentiality on their own. Treat any gateway exposed
beyond an isolated control network as a privileged ingress point.

For production deployments, audit the following:

1. **Network exposure** — bind to a private interface (or behind a reverse
   proxy) instead of `0.0.0.0` whenever possible. Use a host firewall to
   restrict which clients may reach the gateway port.
2. **TLS termination** — for `upstream-ws`, terminate TLS in front of the
   gateway (e.g. nginx, Caddy, Envoy). The gateway speaks plain WebSocket
   so the proxy can offload `wss://` and certificate management.
3. **Origin allowlist** — set `WsGatewayConfig::allowed_origins` to the
   explicit list of browser origins that may connect. An empty list means
   "allow all" and should not be used in production.
4. **Subprotocol enforcement** — set `require_modbus_subprotocol = true`
   so handshakes without `Sec-WebSocket-Protocol: modbus` are rejected
   early.
5. **Session and idle limits** — set `max_sessions` to a value matching
   your expected client count and `idle_timeout` to drop dead/zombie
   sockets.
6. **Downstream rate / scope** — pair the gateway with a routing policy
   that filters unit IDs and (optionally) function codes per session.
7. **Logging / observability** — wire the `tracing` events into your
   log pipeline. The hardened sample at
   [`examples/ws_to_tcp_production.rs`](examples/ws_to_tcp_production.rs)
   shows a complete configuration including signal-driven graceful
   shutdown.
8. **Authentication** — Modbus has none; if you need authn/authz, place
   it at the WebSocket handshake (reverse-proxy basic/OIDC or mTLS) and
   reject unauthenticated upgrades before the gateway sees them.

A complete production-ready WS gateway example with all of the above is
available at
[`mbus-gateway/examples/ws_to_tcp_production.rs`](examples/ws_to_tcp_production.rs).
