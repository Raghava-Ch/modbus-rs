# mbus-async

An async facade for the `modbus-rs` client stack.

`mbus-async` wraps the existing poll-driven `mbus-client` state machine in a Tokio-compatible
`.await` API. You get familiar `async/await` ergonomics without replacing the battle-tested
synchronous protocol core.

## TCP Quick Start

Add to `Cargo.toml`:

```toml
[dependencies]
modbus-rs = { version = "0.4", features = ["async"] }
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
modbus-rs = { version = "0.4", default-features = false, features = [
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
                          │ WorkerCommand (mpsc)
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  Worker thread (std::thread)                                    │
│  - receives WorkerCommand                                       │
│  - calls ClientServices::<_, _, N> sync API                     │
│  - polls state machine in a tight loop                          │
│  - fires oneshot channel when response arrives                  │
└─────────────────────────┬───────────────────────────────────────┘
                          │ Tokio oneshot resolved
                          ▼
┌─────────────────────────────────────────────────────────────────┐
│  Your async code resumes with the typed result                  │
└─────────────────────────────────────────────────────────────────┘
```

Each call gets a unique transaction id. Multiple concurrent calls can be in flight
simultaneously.

TCP uses a compile-time pipeline depth const generic on `AsyncTcpClient<const N: usize = 9>`.
The default is `9` via `AsyncTcpClient::new(...)`, and you can override it at compile time via
`AsyncTcpClient::<N>::new_with_pipeline(...)`.

Serial remains request/reply oriented and defaults to `1` in-flight request.

## Features

| Feature | Default | Enables |
|---|---|---|
| `tcp` | ✓ | `AsyncTcpClient` via `mbus-network` |
| `serial-rtu` | ✓ | `AsyncSerialClient` with RTU framing |
| `serial-ascii` | ✓ | `AsyncSerialClient` with ASCII framing |
| `coils` | ✓ | Coil read/write methods |
| `registers` | ✓ | Register read/write/mask methods |
| `discrete-inputs` | ✓ | Discrete input read methods |
| `fifo` | ✓ | FIFO queue read methods |
| `file-record` | ✓ | File record read/write methods |
| `diagnostics` | ✓ | Device identification, diagnostics, event log, etc. |
| `traffic` | | Dedicated-thread raw TX/RX frame callback API |


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

| Constructor | Pipeline | Poll Interval |
|---|---|---|
| `AsyncTcpClient::new(host, port)` | `9` | default (`20ms`) |
| `AsyncTcpClient::new_with_poll_interval(host, port, interval)` | `9` | custom |
| `AsyncTcpClient::new_with_config(tcp_config, interval)` | `9` | custom |

Custom pipeline (`AsyncTcpClient<N>`) constructors:

| Constructor | Pipeline | Poll Interval |
|---|---|---|
| `AsyncTcpClient::<N>::new_with_pipeline(host, port)` | `N` | default (`20ms`) |
| `AsyncTcpClient::<N>::new_with_pipeline_and_poll_interval(host, port, interval)` | `N` | custom |
| `AsyncTcpClient::<N>::new_with_config_and_pipeline(tcp_config, interval)` | `N` | custom |

## Error Handling

```rust
use modbus_rs::mbus_async::AsyncError;

match client.read_holding_registers(1, 0, 10).await {
    Ok(regs) => { /* use regs */ }
    Err(AsyncError::Mbus(e)) => eprintln!("Modbus error: {}", e),
    Err(AsyncError::WorkerClosed) => eprintln!("worker thread is gone"),
    Err(AsyncError::UnexpectedResponseType) => eprintln!("internal protocol mismatch"),
}
```

## Concurrency

Multiple concurrent `.await` calls are supported. Each call gets an independent
transaction id and Tokio oneshot channel. Responses are routed back to the correct
caller by transaction id when the worker's `AsyncApp` callback fires.

Under TCP the underlying sync client pipelines up to `N` simultaneous requests
(default: `9`).
Under serial, only one request can be outstanding at a time (Modbus serial is
inherently request/reply).

Example with custom TCP pipeline depth:

```rust
use modbus_rs::mbus_async::AsyncTcpClient;

let client = AsyncTcpClient::<16>::new_with_pipeline("127.0.0.1", 502)?;
client.connect().await?;
```

## Traffic Callback (optional `traffic` feature)

Enable `traffic` when you need raw frame observability in async apps:

```toml
[dependencies]
modbus-rs = { version = "0.4", default-features = false, features = [
    "async", "traffic", "tcp", "coils"
] }
tokio = { version = "1", features = ["full"] }
```

```rust,no_run
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
        let client = AsyncTcpClient::new("127.0.0.1", 502)?;

        client.set_traffic_handler(|evt| {
            if let Some(err) = evt.error {
                println!(
                    "[{:?}] txn={} unit={} error={:?} bytes={:02X?}",
                    evt.direction,
                    evt.txn_id,
                    evt.unit_id_slave_addr.get(),
                    err,
                    evt.frame
                );
            } else {
                println!(
                    "[{:?}] txn={} unit={} bytes={:02X?}",
                    evt.direction,
                    evt.txn_id,
                    evt.unit_id_slave_addr.get(),
                    evt.frame
                );
            }
        });

        client.connect().await?;
        let _ = client.read_multiple_coils(1, 0, 8).await?;
        Ok(())
}
```

## License

This crate is licensed under **GPL-3.0-only** — see the repository root [LICENSE](../LICENSE).

If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

