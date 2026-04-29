# WebSocket Gateway (`AsyncWsGatewayServer`)

The `AsyncWsGatewayServer` bridges browser-side WASM clients to downstream
Modbus devices over any async transport (TCP, RTU, ASCII).

---

## Why a WebSocket gateway?

Browsers expose only one low-level socket API: **WebSocket**.  A browser-side
`WasmModbusClient` therefore speaks WebSocket, but downstream Modbus devices
speak raw TCP (Modbus TCP, port 502) or serial RTU/ASCII.  The gateway runs on
a native host that has access to both networks:

```
Browser (WASM)           Gateway (native, Tokio)          Modbus Device(s)
────────────────         ────────────────────────         ────────────────
WasmModbusClient         AsyncWsGatewayServer             Slave Unit 1
  ↕ WebSocket ──────►     WsUpstreamTransport        ──►  TokioTcpTransport
  (MBAP framing)                 ↕ routing
                                 ↓
                         (per unit ID)               ──►  Slave Unit 2
                                                          TokioRtuTransport
```

Because `WasmModbusClient` already constructs complete Modbus TCP ADUs (MBAP
header + PDU) and wraps each one in a binary WebSocket message, the gateway
unwraps each message and forwards the ADU as-is — no re-framing is required on
the upstream side.  The downstream framing (TCP MBAP, RTU CRC16, ASCII LRC) is
handled by the generic `run_async_session` loop shared with
`AsyncTcpGatewayServer`.

---

## Enable the feature

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", features = ["ws-server"] }
```

`ws-server` pulls in `tokio-tungstenite` (opt-in; not included in the default
features).

---

## Minimal example

```rust,no_run
use std::sync::Arc;
use mbus_gateway::{AsyncWsGatewayServer, PassthroughRouter, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(downstream));

    AsyncWsGatewayServer::serve(
        "0.0.0.0:8502",
        WsGatewayConfig::default(),
        PassthroughRouter,
        vec![shared],
    )
    .await?;
    Ok(())
}
```

---

## `WsGatewayConfig`

All security and resource knobs are gathered in a single config struct.  Every
field has a safe default; only fill in what you need.

| Field | Type | Default | Purpose |
|-------|------|---------|---------|
| `idle_timeout` | `Option<Duration>` | `None` | Drop sessions silent for longer than this duration (guards against crashed browser tabs). |
| `max_sessions` | `usize` | `0` = unlimited | Reject the N+1th connection at the WS handshake stage. |
| `require_modbus_subprotocol` | `bool` | `false` | Require `Sec-WebSocket-Protocol: modbus` header; reject HTTP 400 otherwise. |
| `allowed_origins` | `Vec<String>` | `[]` = allow all | CORS allowlist — reject HTTP 403 if the `Origin` header is not in this list. |

### Example: all options

```rust,no_run
use mbus_gateway::WsGatewayConfig;
use std::time::Duration;

let config = WsGatewayConfig {
    idle_timeout: Some(Duration::from_secs(30)),
    max_sessions: 64,
    require_modbus_subprotocol: true,
    allowed_origins: vec!["https://hmi.example.com".to_string()],
};
```

---

## Multi-downstream routing

Connect one WebSocket gateway to several downstream devices and route by
unit ID.  The index in the `downstreams` Vec corresponds to the channel
index returned by the routing policy.

```rust,no_run
use std::sync::Arc;
use mbus_gateway::{AsyncWsGatewayServer, RangeRouteTable, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let device_a = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let device_b = TokioTcpTransport::connect("192.168.1.11:502").await?;

    // Units 1–10 → channel 0 (device A), units 11–20 → channel 1 (device B).
    let mut router: RangeRouteTable<4> = RangeRouteTable::new();
    router.add(1, 10, 0).unwrap();
    router.add(11, 20, 1).unwrap();

    AsyncWsGatewayServer::serve(
        "0.0.0.0:8502",
        WsGatewayConfig::default(),
        router,
        vec![Arc::new(Mutex::new(device_a)), Arc::new(Mutex::new(device_b))],
    )
    .await?;
    Ok(())
}
```

---

## RTU serial downstream

The downstream can be any `AsyncTransport` — including Tokio async serial.
Pair `ws-server` with the `serial-rtu` feature:

```toml
mbus-gateway = { version = "0.8.0", features = ["ws-server", "serial-rtu"] }
```

```rust,no_run
use std::sync::Arc;
use std::time::Duration;
use mbus_core::transport::{BaudRate, ModbusConfig, Parity, SerialConfig, UnitIdOrSlaveAddr};
use mbus_gateway::{AsyncWsGatewayServer, UnitRouteTable, WsGatewayConfig};
use mbus_serial::TokioRtuTransport;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let serial_cfg = ModbusConfig::Serial(
        SerialConfig::builder()
            .port("/dev/ttyUSB0")
            .baud_rate(BaudRate::Baud19200)
            .parity(Parity::None)
            .build()?,
    );
    let rtu = TokioRtuTransport::new(&serial_cfg)?;

    let mut router: UnitRouteTable<8> = UnitRouteTable::new();
    for unit in 1u8..=8 {
        router.add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0).unwrap();
    }

    AsyncWsGatewayServer::serve(
        "0.0.0.0:8502",
        WsGatewayConfig {
            idle_timeout: Some(Duration::from_secs(60)),
            max_sessions: 8,
            ..WsGatewayConfig::default()
        },
        router,
        vec![Arc::new(Mutex::new(rtu))],
    )
    .await?;
    Ok(())
}
```

---

## Graceful shutdown

Use `serve_with_shutdown` to stop accepting new connections on a signal
while letting in-flight sessions complete:

```rust,no_run
use mbus_gateway::{AsyncWsGatewayServer, PassthroughRouter, WsGatewayConfig};
use mbus_network::TokioTcpTransport;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(downstream));

    let shutdown = async {
        tokio::signal::ctrl_c().await.unwrap();
        println!("Shutting down…");
    };

    AsyncWsGatewayServer::serve_with_shutdown(
        "0.0.0.0:8502",
        WsGatewayConfig::default(),
        PassthroughRouter,
        vec![shared],
        shutdown,
    )
    .await?;
    Ok(())
}
```

---

## Security recommendations

For any deployment where the WebSocket port is reachable from untrusted
networks:

1. **Enable subprotocol enforcement** (`require_modbus_subprotocol: true`) to
   prevent accidental browser navigation from consuming a session slot.
2. **Set an `allowed_origins` allowlist** containing only your known HMI
   origins.  An empty list allows all origins.
3. **Set `idle_timeout`** to a value appropriate for your HMI's polling
   interval (e.g., 3× the maximum expected silence between requests).  30–60
   seconds is reasonable for most PLC polling scenarios.
4. **Set `max_sessions`** to a value proportional to your downstream device
   capacity.  RTU buses that serialise traffic should use a small cap (e.g.,
   8–16); TCP device trees can handle more.
5. **Consider TLS (WSS)** for traffic that crosses untrusted networks.  The
   existing `serve` / `serve_with_shutdown` methods accept raw TCP; a TLS
   terminating reverse proxy (nginx, haproxy) in front of the gateway is the
   simplest path.

---

## How it works internally

`AsyncWsGatewayServer` reuses the same generic `run_async_session` loop as
`AsyncTcpGatewayServer`.  The only difference is the upstream transport:

| Server | Upstream transport | Source |
|--------|--------------------|--------|
| `AsyncTcpGatewayServer` | raw `TcpStream` | kernel TCP stack |
| `AsyncWsGatewayServer` | `WsUpstreamTransport` wrapping `WebSocketStream<TcpStream>` | `tokio-tungstenite` |

`WsUpstreamTransport` implements `AsyncTransport` with
`TRANSPORT_TYPE = CustomTcp`, meaning the session loop treats the ADU bytes
identically to raw TCP — MBAP framing is used throughout.  The binary
WebSocket envelope is just the carrier.

### Handshake sequence per connection

1. OS accepts the TCP connection (`TcpListener::accept`).
2. Gateway checks the semaphore; rejects if `max_sessions` is reached.
3. `tokio_tungstenite::accept_hdr_async` upgrades the connection to WebSocket,
   validating `Origin` and `Sec-WebSocket-Protocol` headers in a callback.
4. The resulting `WebSocketStream` is wrapped in `WsUpstreamTransport`.
5. If `idle_timeout` is set, the transport is further wrapped in
   `IdleTimeoutTransport`.
6. `run_async_session(upstream, router, downstreams)` runs the standard
   Modbus gateway loop for the duration of the session.

---

## Runnable examples

| Example | Command |
|---------|---------|
| Basic WS → TCP | `cargo run --example ws_to_tcp --features ws-server -p mbus-gateway` |
| Production (security + shutdown) | `cargo run --example ws_to_tcp_production --features ws-server -p mbus-gateway` |
| Multi-downstream routing | `cargo run --example ws_to_multi_downstream --features ws-server -p mbus-gateway` |
| WS → RTU serial | `cargo run --example ws_to_rtu --features ws-server,serial-rtu -p mbus-gateway` |
