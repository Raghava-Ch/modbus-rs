# mbus-async

A pure-async Tokio facade for Modbus client communication, with optional async server adapters.

`mbus-async` drives Modbus communication natively in Tokio tasks. Each request is a `Future`
that resolves when the server responds. The transport layer is owned by a background Tokio task
and communicates with the public API through Tokio channels (`mpsc`, `oneshot`, and `watch`).

## TCP Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

```rust
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AsyncTcpClient::new("192.168.1.10", 502)?;
    client.connect().await?;

    // Read 10 holding registers starting at address 0
    let regs = client.read_holding_registers(1, 0, 10).await?;
    for addr in regs.from_address()..regs.from_address() + regs.quantity() {
        println!("reg[{}] = {}", addr, regs.value(addr)?);
    }

    // Write multiple registers
    let (start, qty) = client.write_multiple_registers(1, 0, &[100, 200, 300]).await?;
    println!("Wrote {} registers starting at {}", qty, start);

    Ok(())
}
```

## Serial Quick Start

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = [
    "async", "serial-rtu", "coils", "registers"
] }
tokio = { version = "1", features = ["full"] }
```

```rust
use modbus_rs::mbus_async::AsyncSerialClient;
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
};
use std::str::FromStr;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str("/dev/ttyUSB0").unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 2000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };

    let client = AsyncSerialClient::new_rtu(config)?;
    client.connect().await?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("coil[{}] = {}", addr, coils.value(addr)?);
    }

    Ok(())
}
```

## Design

```
┌─────────────────────────────────────────────────────────────────┐
│  Your async code                                                │
│  client.read_holding_registers(1, 0, 10).await?                 │
└─────────────────────────┬───────────────────────────────────────┘
                          │ TaskCommand::Request (mpsc)
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  ClientTask  (Tokio task — tokio::spawn)                        │
│                                                                 │
│  tokio::select! {                                               │
│    frame  ← transport.recv_frame()   (TCP / serial)             │
│    cmd    ← mpsc::Receiver<TaskCommand>                         │
│  }                                                              │
│                                                                 │
│  pending: HashMap<txn_id, (request, oneshot::Sender)>           │
│  queued:  VecDeque<TaskCommand>   (if in_flight == N)           │
└─────────────────────────┬───────────────────────────────────────┘
                          │ oneshot resolved
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  Your async code resumes with the typed result                  │
└─────────────────────────────────────────────────────────────────┘
```

1. `new()` / `new_rtu()` creates a client and spawns a Tokio task (`tokio::spawn(task.run())`).
2. Each request method creates a `oneshot` channel, enqueues a `TaskCommand::Request`, and
   `await`s the oneshot receiver.
3. The task drives `tokio::select!` between receiving frames from the transport and receiving
   new commands from the public API.
4. On a complete response frame the task matches it to the pending entry by transaction id and
   resolves the caller's oneshot.
5. `has_pending_requests()` is a **synchronous** check (reads a `watch` channel — no `.await`).
6. Dropping all handles closes the mpsc channel; the task exits cleanly.

TCP uses a compile-time pipeline depth `N` (`AsyncTcpClient<const N: usize = 9>`).  
Serial is always depth 1 (request/reply protocol).


## Features

| Feature | Default | Enables |
|---|---|---|
| `network-tcp` | ✓ | `AsyncTcpClient` via `mbus-network` |
| `serial-rtu` |  | `AsyncSerialClient` with RTU framing |
| `serial-ascii` |  | `AsyncSerialClient` with ASCII framing |
| `coils` | ✓ | Coil read/write methods |
| `registers` | ✓ | Register read/write/mask methods |
| `discrete-inputs` | ✓ | Discrete input read methods |
| `fifo` | ✓ | FIFO queue read methods |
| `file-record` | ✓ | File record read/write methods |
| `diagnostics` | ✓ | Device identification, diagnostics, event log, etc. |
| `traffic` |  | Raw TX/RX frame callback API from the background async task |
| `diagnostics-stats` |  | Async server diagnostics counters (depends on `diagnostics`) |
| `logging` |  | Enables `log` integration in this crate |
| `server-tcp` |  | `server::AsyncTcpServer` via `mbus-network` async transport |
| `server-serial` |  | `server::AsyncRtuServer` and `server::AsyncAsciiServer` |
| `full` |  | `default` + `traffic` |


## Available Methods

### `AsyncTcpClient` and `AsyncSerialClient`

Constructors are side-effect free. Build the client first, then call
`client.connect().await?` before issuing Modbus requests.

Both clients expose an identical async API:

| Method | FC | Feature |
|---|---|---|
| `read_multiple_coils(unit, address, quantity)` | 01 | `coils` |
| `write_single_coil(unit, address, value)` | 05 | `coils` |
| `write_multiple_coils(unit, address, &coils)` | 15 | `coils` |
| `read_discrete_inputs(unit, address, quantity)` | 02 | `discrete-inputs` |
| `read_holding_registers(unit, address, quantity)` | 03 | `registers` |
| `read_input_registers(unit, address, quantity)` | 04 | `registers` |
| `write_single_register(unit, address, value)` | 06 | `registers` |
| `write_multiple_registers(unit, address, &values)` | 16 | `registers` |
| `read_write_multiple_registers(unit, ra, rq, wa, &wv)` | 23 | `registers` |
| `mask_write_register(unit, address, and_mask, or_mask)` | 22 | `registers` |
| `read_fifo_queue(unit, address)` | 24 | `fifo` |
| `read_file_record(unit, &sub_request)` | 20 | `file-record` |
| `write_file_record(unit, &sub_request)` | 21 | `file-record` |
| `read_device_identification(unit, code, object_id)` | 43/14 | `diagnostics` |
| `encapsulated_interface_transport(unit, mei, &data)` | 43 | `diagnostics` |
| `read_exception_status(unit)` | 07 | `diagnostics` |
| `diagnostics(unit, sub_fn, &data)` | 08 | `diagnostics` |
| `get_comm_event_counter(unit)` | 11 | `diagnostics` |
| `get_comm_event_log(unit)` | 12 | `diagnostics` |
| `report_server_id(unit)` | 17 | `diagnostics` |

### Serial-specific constructors

| Constructor | Mode |
|---|---|
| `AsyncSerialClient::new_rtu(config)` | RTU |
| `AsyncSerialClient::new_rtu_with_poll_interval(config, interval)` | RTU |
| `AsyncSerialClient::new_ascii(config)` | ASCII |
| `AsyncSerialClient::new_ascii_with_poll_interval(config, interval)` | ASCII |

Each constructor validates that `ModbusSerialConfig::mode` matches the constructor's expected mode, returning `AsyncError::Mbus(MbusError::InvalidConfiguration)` on mismatch.

### TCP-specific constructors

Default pipeline (`AsyncTcpClient<9>`) constructors:

| Constructor | Notes |
|---|---|
| `AsyncTcpClient::new(host, port)` | Pipeline depth 9 |
| `AsyncTcpClient::new_with_poll_interval(host, port, interval)` | `poll_interval` ignored (pure-async) |
| `AsyncTcpClient::new_with_config(tcp_config, interval)` | Full `ModbusTcpConfig` |

Custom pipeline (`AsyncTcpClient<N>`) constructors:

| Constructor | Notes |
|---|---|
| `AsyncTcpClient::<N>::new_with_pipeline(host, port)` | Compile-time depth `N` |
| `AsyncTcpClient::<N>::new_with_pipeline_and_poll_interval(host, port, interval)` | `poll_interval` ignored |
| `AsyncTcpClient::<N>::new_with_config_and_pipeline(tcp_config, interval)` | Full config + depth `N` |

## Error Handling

<!-- validate: skip -->
```rust
use modbus_rs::mbus_async::AsyncError;

match client.read_holding_registers(1, 0, 10).await {
    Ok(regs) => { /* use regs */ }
    Err(AsyncError::Mbus(e)) => eprintln!("Modbus error: {e}"),
    Err(AsyncError::WorkerClosed) => eprintln!("background task exited"),
    Err(AsyncError::UnexpectedResponseType) => eprintln!("internal routing mismatch"),
    Err(AsyncError::Timeout) => eprintln!("per-request timeout elapsed"),
}
```

## Concurrency

Multiple concurrent `.await` calls are supported. Each gets an independent transaction id
and `oneshot` channel. The background Tokio task routes responses back by transaction id.

- **TCP**: up to `N` requests in-flight simultaneously (default `N = 9`). Excess requests
  are queued internally and dispatched as pipeline slots free up.
- **Serial**: exactly 1 in-flight request (Modbus serial is request/reply).

Example with custom TCP pipeline depth:

<!-- validate: skip -->
```rust
use modbus_rs::mbus_async::AsyncTcpClient;

let client = AsyncTcpClient::<16>::new_with_pipeline("127.0.0.1", 502)?;
client.connect().await?;
```

## Per-Request Timeout

Set a deadline applied to all subsequent requests:

<!-- validate: skip -->
```rust
use std::time::Duration;

client.set_request_timeout(Duration::from_millis(500));

// Returns AsyncError::Timeout after 500ms with no response.
// The background task automatically drains the pipeline and closes the
// transport — call client.connect().await? to recover.
let result = client.read_multiple_coils(1, 0, 8).await;

client.clear_request_timeout(); // back to "wait forever"
```

## Checking Pending Requests

`has_pending_requests()` is **synchronous** — no `.await` required:

<!-- validate: skip -->
```rust
if client.has_pending_requests() {
    println!("requests still in flight");
}
```

## Reconnect

After a transport error or timeout, call `connect()` again to restore the connection:

<!-- validate: skip -->
```rust
client.connect().await?; // safe to call repeatedly
```

## Traffic Callback (optional `traffic` feature)

Enable `traffic` when you need raw frame observability in async apps:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = [
    "async", "traffic", "network-tcp", "coils"
] }
tokio = { version = "1", features = ["full"] }
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
    fn on_tx_error(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, error: MbusError, frame: &[u8]) {
        println!("[TX ERR] txn={txn_id} unit={} error={error:?} bytes={frame:02X?}", unit.get());
    }
    fn on_rx_error(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, error: MbusError, frame: &[u8]) {
        println!("[RX ERR] txn={txn_id} unit={} error={error:?} bytes={frame:02X?}", unit.get());
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AsyncTcpClient::new("127.0.0.1", 502)?;
    #[cfg(feature = "traffic")]
    client.set_traffic_notifier(FrameLogger);
    client.connect().await?;
    let _ = client.read_multiple_coils(1, 0, 8).await?;
    Ok(())
}
```

## License

This crate is licensed under **GPL-3.0** — see the repository root [LICENSE](../LICENSE).

If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

