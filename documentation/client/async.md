# Async Client Development

Using Tokio-based async clients for Modbus TCP and Serial.

---

## Prerequisites

Enable the `async` feature:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", features = ["async"] }
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

`AsyncSerialClient` supports RTU and ASCII constructors:

- `AsyncSerialClient::new_rtu(config)`
- `AsyncSerialClient::new_ascii(config)`
- `AsyncSerialClient::new_rtu_with_poll_interval(config, interval)`
- `AsyncSerialClient::new_ascii_with_poll_interval(config, interval)`

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

The async client is a pure-Tokio transport driver — no worker threads, no poll loops:

```
┌──────────────────────────────────────────────────────────┐
│  AsyncTcpClient / AsyncSerialClient                      │
│  └── AsyncClientCore                                     │
│        ├── mpsc::Sender<TaskCommand>  ─────────────────┐ │
│        ├── watch::Receiver<usize>  (pending count)     │ │
│        └── Arc<AtomicU64>          (timeout ns)        │ │
└────────────────────────────────────────────────────────│─┘
                                                         │
                                          tokio::spawn   ▼
┌──────────────────────────────────────────────────────────┐
│  ClientTask<T, N>  (background Tokio task)               │
│                                                          │
│  tokio::select! {                                        │
│    frame ← transport.recv_frame()                        │
│    cmd   ← mpsc::Receiver<TaskCommand>                   │
│  }                                                       │
│                                                          │
│  pending: HashMap<txn_id, (request, oneshot::Sender)>    │
│  queued:  VecDeque  (overflow when in_flight == N)       │
└──────────────────────────────────────────────────────────┘
```

### How It Works

1. `new()` / `new_rtu()` spawns a Tokio task (`tokio::spawn(task.run())`). No threads are created.
2. Each request method creates a `oneshot` channel, sends a `TaskCommand::Request` carrying the
   oneshot sender over the mpsc channel, and `await`s the oneshot receiver.
3. The Tokio task runs `tokio::select!` between receiving frames from the transport and
   receiving new commands from the public API.
4. On a complete response frame the task matches it by transaction id to the pending entry and
   resolves the caller's oneshot with the typed result.
5. `has_pending_requests()` is **synchronous** — it reads a `watch` channel; no `.await` needed.
6. Dropping all client handles closes the mpsc channel; the task's `recv()` returns `None`
   and the task exits cleanly.

### Pipelining

The `N` const generic controls how many requests may be in-flight simultaneously.

- **TCP**: default `N = 9` (`AsyncTcpClient<9>`). Requests beyond `N` are queued locally
  and dispatched as pipeline slots become available.
- **Serial**: always `N = 1` — Modbus serial is a strict request/reply protocol.

### Drop Behavior

When all `AsyncTcpClient` / `AsyncSerialClient` handles are dropped, the mpsc sender closes.
The background Tokio task exits on the next iteration. Any in-flight `Future`s that are still
being awaited resolve with `AsyncError::WorkerClosed`.

---

## Checking Pending Requests

`has_pending_requests()` is synchronous — no `.await` required:

<!-- validate: skip -->
```rust
if client.has_pending_requests() {
    println!("requests still in flight");
}
```

---

## Per-Request Timeout

Set a deadline applied to every subsequent request:

<!-- validate: skip -->
```rust
use std::time::Duration;

client.set_request_timeout(Duration::from_millis(500));

// AsyncError::Timeout is returned if no response arrives within 500ms.
// The pipeline is automatically drained and the transport closed.
client.clear_request_timeout(); // remove the deadline
```

After a timeout, call `client.connect().await?` to reopen the transport.

---

## Reconnect After Disconnect

`connect()` is safe to call at any time — it closes any active transport first, then opens a
new connection. Use it after a transport error or `AsyncError::Timeout`:

<!-- validate: skip -->
```rust
if let Err(_) = client.read_multiple_coils(1, 0, 8).await {
    client.connect().await?; // reconnect and retry
    let _ = client.read_multiple_coils(1, 0, 8).await?;
}
```

---

## Traffic Observability (Async)

Enable with `traffic` feature:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", features = ["async", "traffic"] }
```

```rust
use modbus_rs::mbus_async::AsyncTcpClient;
#[cfg(feature = "traffic")]
use modbus_rs::mbus_async::AsyncClientNotifier;
use modbus_rs::{MbusError, UnitIdOrSlaveAddr};

#[cfg(feature = "traffic")]
struct FrameLogger;
#[cfg(feature = "traffic")]
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
    #[cfg(feature = "traffic")]
    client.set_traffic_notifier(FrameLogger);
    client.connect().await?;
    Ok(())
}
```

See [modbus-rs/examples/client/network-tcp/async/traffic.rs](../../modbus-rs/examples/client/network-tcp/async/traffic.rs)

---

## Async API Coverage

Both `AsyncTcpClient` and `AsyncSerialClient` expose the same request API (feature-gated):

- Coils: FC01, FC05, FC0F
- Discrete inputs: FC02
- Registers: FC03, FC04, FC06, FC10, FC16, FC17
- FIFO: FC18 (`fifo` feature)
- File record: FC14, FC15 (`file-record` feature)
- Diagnostics: FC07, FC08, FC0B, FC0C, FC11, FC2B (`diagnostics` feature)

---

## See Also

- [Sync Development](sync.md) — Poll-driven sync client details
- [Feature Flags](feature_flags.md) — Enable `async` feature
- [Architecture](architecture.md) — Internal design
