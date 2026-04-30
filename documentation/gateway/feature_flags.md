# Gateway Feature Flags

## `mbus-gateway` crate features

| Feature | Default | Requires std | Description |
|---------|---------|-------------|-------------|
| `async` | ✓ | Yes | Async Tokio gateway (`AsyncTcpGatewayServer`), pulls in `mbus-network/async` and `tokio` |
| `ws-server` | ✗ | Yes | WebSocket gateway (`AsyncWsGatewayServer`) for WASM clients; adds `tokio-tungstenite` |
| `logging` | ✓ | Yes | `log` crate integration — gateway activity logged at `debug`/`trace` level |
| `network` | ✗ | Yes | Re-exports `StdTcpTransport` + `StdTcpServerTransport` from `mbus-network` for sync TCP use |
| `serial-rtu` | ✗ | Yes | Re-exports `StdRtuTransport` from `mbus-serial` for sync RTU serial use |
| `serial-ascii` | ✗ | Yes | Re-exports `StdAsciiTransport` from `mbus-serial` for sync ASCII serial use |
| `traffic` | ✗ | No | Enables `on_upstream_rx` and `on_downstream_tx` in `GatewayEventHandler` |
| `std-required` | (internal) | — | Internal sentinel; implied by `async`, `logging`, `ws-server`, and all transport features |

## How features affect `no_std` support

The `lib.rs` of `mbus-gateway` gates on `std-required`:

```rust
#![cfg_attr(not(any(doc, feature = "std-required")), no_std)]
```

When only `traffic` is enabled (no `async`, no `ws-server`, no `logging`), the crate is fully
`no_std` and can be used on bare-metal targets without an allocator.

## Disabling defaults (embedded/no_std)

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", default-features = false }
```

This compiles only the sync gateway core:
- `GatewayServices`, `UnitRouteTable`, `RangeRouteTable`, `PassthroughRouter`,
  `UnitIdRewriteRouter`, `TxnMap`, `GatewayEventHandler`, `NoopEventHandler`.
- No Tokio, no `log`, no `std`.

## Enabling `traffic` callbacks

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", default-features = false, features = ["traffic"] }
```

This adds `on_upstream_rx` and `on_downstream_tx` to `GatewayEventHandler` so
you can capture the raw bytes for debugging or protocol analysis.

## Enabling the WebSocket gateway

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", features = ["ws-server"] }
```

This adds `AsyncWsGatewayServer` and `WsGatewayConfig` to the public API and
pulls in `tokio-tungstenite` as a dependency.  The downstream side is
unchanged — any `AsyncTransport` (TCP, RTU, ASCII) can still be used.

To bridge WebSocket clients to an async RTU bus, combine both features:

```toml
mbus-gateway = { version = "0.8.0", features = ["ws-server", "serial-rtu"] }
```

## `modbus-rs` top-level crate

The `gateway` feature pulls in `mbus-gateway` with its defaults:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", features = ["gateway"] }
```

The re-export is at `modbus_rs::gateway` (i.e. `modbus_rs::gateway::GatewayServices`, etc.).
