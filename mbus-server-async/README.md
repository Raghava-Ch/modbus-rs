# mbus-server-async

Role-focused async Modbus server facade for the modbus-rs workspace.

This crate re-exports the server API from `mbus-async::server` and keeps the
existing `mbus-async` crate unchanged for backward compatibility.

## Positioning

- Prefer this crate when you want an async server-only dependency.
- `mbus-async` remains supported as the combined async client+server crate.

## Usage

```toml
[dependencies]
mbus-server-async = "0.8.0"
```

```rust
use mbus_server_async::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};

struct App;

impl AsyncAppHandler for App {
    async fn handle(&mut self, _req: ModbusRequest) -> ModbusResponse {
        ModbusResponse::NoResponse
    }
}
```

Run the included quick-start example:

```bash
cargo run -p mbus-server-async --example quick_async_server --features server-tcp,coils,registers
```

Run the macro-based server example (`#[async_modbus_app]`):

```bash
cargo run -p mbus-server-async --example macro_async_server --features server-tcp,coils,registers
```

## Features

This crate forwards all of its feature flags to `mbus-async` server-related
features (for example: `server-tcp`, `server-serial`, `diagnostics-stats`,
`traffic`).
