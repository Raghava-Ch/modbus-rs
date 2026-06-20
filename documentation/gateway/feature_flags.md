# Gateway Feature Flags

## `mbus-gateway` crate features

The `mbus-gateway` crate features are modularly separated into core engine features, upstream server capabilities, and downstream client connections.

| Feature | Default | Requires std | Description |
|---------|---------|-------------|-------------|
| **Core Features** | | | |
| `async` | ✓ | Yes | Async Tokio gateway runtime support, including async servers (`AsyncTcpGatewayServer`, `AsyncSerialGatewayServer`, etc.) |
| `logging` | ✓ | Yes | `log` crate integration — gateway activity logged at `debug`/`trace`/`warn` levels |
| `traffic` | ✗ | No | Enables detailed `on_upstream_rx`, `on_upstream_tx`, `on_downstream_rx`, and `on_downstream_tx` callbacks in `GatewayEventHandler` |
| `std-required` | (internal) | — | Internal sentinel; automatically implied by any feature that requires std (e.g., all network or async options) |
| **Upstream Features** | | | |
| `upstream-tcp` | ✓ | Yes | Upstream TCP server transport support (`AsyncTcpGatewayServer`) |
| `upstream-ws` | ✓ | Yes | Upstream WebSocket gateway (`AsyncWsGatewayServer`) for WASM/browser clients; adds `tokio-tungstenite` |
| `upstream-serial-rtu` | ✓ | Yes | Upstream Modbus RTU serial server support (`AsyncSerialGatewayServer`) |
| `upstream-serial-ascii` | ✓ | Yes | Upstream Modbus ASCII serial server support (`AsyncSerialGatewayServer`) |
| **Downstream Features** | | | |
| `downstream-tcp` | ✓ | Yes | Downstream TCP client support (`StdTcpTransport` and `TokioTcpTransport`) |
| `downstream-serial-rtu` | ✓ | Yes | Downstream RTU serial client support (`StdRtuTransport` and `TokioRtuTransport`) |
| `downstream-serial-ascii`| ✓ | Yes | Downstream ASCII serial client support (`StdAsciiTransport` and `TokioAsciiTransport`) |

## How features affect `no_std` support

The `lib.rs` of `mbus-gateway` gates on `std-required`:

```rust
#![cfg_attr(not(any(doc, feature = "std-required")), no_std)]
```

When all std-requiring features are disabled (i.e. only `traffic` is enabled, or no features are selected), the crate is fully `no_std` compatible and can run on bare-metal MCU platforms without an allocator.

## Disabling defaults (embedded/no_std)

To use the gateway in a bare-metal `no_std` environment, specify `default-features = false`:

```toml
[dependencies]
mbus-gateway = { version = "0.15.0", default-features = false }
```

This compiles only the synchronous, poll-driven gateway core:
- `GatewayServices`, `UnitRouteTable`, `RangeRouteTable`, `PassthroughRouter`, `UnitIdRewriteRouter`, `TxnMap`, `GatewayEventHandler`, `NoopEventHandler`.
- Zero dependency on Tokio, no `log`, and no `std`.

## Enabling `traffic` callbacks in no_std

If you want traffic inspection callbacks but still want to stay `no_std`:

```toml
[dependencies]
mbus-gateway = { version = "0.15.0", default-features = false, features = ["traffic"] }
```

This activates `on_upstream_rx`/`on_downstream_tx` callbacks in your custom `GatewayEventHandler` to let you capture raw frames.

## Customizing Async/Std features

If you are running in a `std` environment (like Linux/macOS/Windows) and want to only enable specific transports (e.g. WebSocket upstream bridging to a TCP downstream device) to keep your compile times or binary size small, configure the features explicitly:

```toml
[dependencies]
mbus-gateway = { version = "0.15.0", default-features = false, features = ["upstream-ws", "downstream-tcp"] }
```

## `modbus-rs` top-level crate

When using the umbrella `modbus-rs` crate, the `gateway` feature pulls in `mbus-gateway` with all its standard defaults:

```toml
[dependencies]
modbus-rs = { version = "0.15.0", features = ["gateway"] }
```

The gateway types are fully re-exported at the top-level path `modbus_rs::gateway` (e.g., `modbus_rs::gateway::GatewayServices`, `modbus_rs::gateway::AsyncTcpGatewayServer`, etc.).

