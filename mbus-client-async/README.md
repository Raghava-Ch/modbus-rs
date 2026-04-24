# mbus-client-async

Role-focused async Modbus client facade for the modbus-rs workspace.

This crate re-exports the client API from `mbus-async::client` and keeps the
existing `mbus-async` crate unchanged for backward compatibility.

## Positioning

- Prefer this crate when you want an async client-only dependency.
- `mbus-async` remains supported as the combined async client+server crate.

## Usage

```toml
[dependencies]
mbus-client-async = "0.8.0"
```

```rust
use mbus_client_async::AsyncTcpClient;

# async fn demo() -> anyhow::Result<()> {
let client = AsyncTcpClient::new("127.0.0.1", 502)?;
client.connect().await?;
# Ok(())
# }
```

Run the included quick-start example:

```bash
cargo run -p mbus-client-async --example quick_async_client --features network-tcp,coils
```

## Features

This crate forwards all of its feature flags to `mbus-async` client-related
features (for example: `network-tcp`, `serial-rtu`, `coils`, `registers`,
`traffic`).
