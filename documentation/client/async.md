# Async Client Development

Using Tokio-based async clients for Modbus TCP and Serial.

---

## Prerequisites

Enable the `async` feature:

```toml
[dependencies]
modbus-rs = { version = "0.6.0", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

---

## Async TCP Client

### Basic Usage

```rust
use anyhow::Result;
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let client = AsyncTcpClient::new("192.168.1.10", 502)?;
    client.connect().await?;

    // Each request returns a Future — await it directly for the result
    let coils = client.read_multiple_coils(1, 0, 16).await?;
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("coil[{}] = {}", addr, coils.value(addr)?);
    }

    let holding = client.read_holding_registers(1, 0, 4).await?;
    for addr in holding.from_address()..holding.from_address() + holding.quantity() {
        println!("reg[{}] = {}", addr, holding.value(addr)?);
    }

    Ok(())
}
```

### Full Example

See [modbus-rs/examples/client/network-tcp/async/tcp.rs](../../modbus-rs/examples/client/network-tcp/async/tcp.rs)

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --features async
```

---

## Async Serial Client

### RTU Mode

```rust
use anyhow::{anyhow, Result};
use modbus_rs::mbus_async::AsyncSerialClient;
use modbus_rs::{
    BaudRate, DataBits, ModbusSerialConfig, Parity, SerialMode,
    BackoffStrategy, JitterStrategy,
};

#[tokio::main]
async fn main() -> Result<()> {
    let config = ModbusSerialConfig {
        port_path: "/dev/ttyUSB0"
            .try_into()
            .map_err(|_| anyhow!("serial port path exceeds 64 bytes"))?,
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Baud19200,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::Even,
        response_timeout_ms: 1000,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };

    let client = AsyncSerialClient::new_rtu(config)?;
    client.connect().await?;

    let holding = client.read_holding_registers(1, 0, 10).await?;
    for addr in holding.from_address()..holding.from_address() + holding.quantity() {
        println!("reg[{}] = {}", addr, holding.value(addr)?);
    }

    Ok(())
}
```

### Full Example

See [modbus-rs/examples/client/serial-rtu/async/rtu.rs](../../modbus-rs/examples/client/serial-rtu/async/rtu.rs)

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_serial_rtu \
    --no-default-features --features async,serial-rtu,coils,registers
```

---

## Architecture

The async client wraps the synchronous `ClientServices` with a Tokio runtime:

```
┌──────────────────────────────────────────────────────────┐
│  AsyncTcpClient / AsyncSerialClient                      │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Arc<Mutex<ClientServices>>                        │  │
│  └────────────────────────────────────────────────────┘  │
│                          ▲                               │
│                          │                               │
│  ┌──────────────────────────────────────────────────┐    │
│  │  Internal worker thread                          │    │
│  │  - Calls client.poll() in a loop                 │    │
│  │  - Resolves per-request Futures on response      │    │
│  └──────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────┘
```

### How It Works

1. `new()` / `new_rtu()` creates the client and an internal worker thread that drives `ClientServices::poll()`
2. Each request method (e.g. `read_multiple_coils`) sends a command to the worker and returns a `Future`
3. The worker processes the response and resolves the `Future` — no callbacks involved
4. The worker is event-driven when idle: it blocks when there are no in-flight requests and wakes on new commands.
5. During active traffic it polls at the configured `poll_interval`.
6. `.await`-ing the `Future` gives you the response value directly, or an `AsyncError` on failure

You can query worker state explicitly:

```rust
let has_pending = client.has_pending_requests().await?;
println!("pending={has_pending}");
```

### Drop Behavior

When the async client is dropped, the worker thread stops. Any in-flight `Future`s resolve with `AsyncError::WorkerClosed`.

---

## Traffic Observability (Async)

Enable with `traffic` feature:

```toml
[dependencies]
modbus-rs = { version = "0.6.0", features = ["async", "traffic"] }
```

```rust,no_run
use modbus_rs::mbus_async::{AsyncClientNotifier, AsyncTcpClient};
use modbus_rs::{MbusError, UnitIdOrSlaveAddr};

struct FrameLogger;
impl AsyncClientNotifier for FrameLogger {
    fn on_tx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("[TX] txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
    }
    fn on_rx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("[RX] txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AsyncTcpClient::new("192.168.1.10", 502)?;
    client.set_traffic_notifier(FrameLogger);
    client.connect().await?;
    Ok(())
}
```

See [modbus-rs/examples/client/network-tcp/async/traffic.rs](../../modbus-rs/examples/client/network-tcp/async/traffic.rs)

---

## See Also

- [Building Applications](building_applications.md) — Sync client details
- [Feature Flags](feature_flags.md) — Enable `async` feature
- [Architecture](architecture.md) — Internal design
