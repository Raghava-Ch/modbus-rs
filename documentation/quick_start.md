# Modbus-rs Quick Start Guide

This guide shows the current way to start with `modbus-rs` using the top-level crate and
its feature flags.

For detailed feature combinations, see [feature_flags.md](feature_flags.md).

## 1. Choose Your Transport and Features

`modbus-rs` is the recommended top-level dependency for most users.

### Full default stack

```toml
[dependencies]
modbus-rs = "0.4.0"
```

This enables:
- client services
- TCP transport
- Serial RTU transport
- Serial ASCII transport
- all supported function-group features

### Minimal TCP client

```toml
[dependencies]
modbus-rs = { version = "0.4.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

### Minimal Serial RTU client

```toml
[dependencies]
modbus-rs = { version = "0.4.0", default-features = false, features = [
  "client",
  "serial-rtu",
  "registers"
] }
```

### Minimal Serial ASCII client

```toml
[dependencies]
modbus-rs = { version = "0.4.0", default-features = false, features = [
  "client",
  "serial-ascii",
  "coils"
] }
```

### WASM browser setup (independent `mbus-ffi` crate)

WASM support is provided by `mbus-ffi` directly.
`modbus-rs` does not provide a top-level `wasm` feature or WASM type re-exports.

For the workspace browser smoke pages, build the `mbus-ffi` package used by the HTML pages:

```bash
cd /path/to/modbus-rs/mbus-ffi
wasm-pack build --target web --features wasm,full
python3 -m http.server 8089 --directory ./examples/
```

Open:

- `http://localhost:8089/network_smoke.html`
- `http://localhost:8089/serial_smoke.html`

Run WASM browser feature tests:

```bash
cd /path/to/modbus-rs/mbus-ffi;
wasm-pack test --chrome --target wasm32-unknown-unknown --features wasm,full
```

## 2. Basic Usage Example (TCP)

This example uses the current `ClientServices::new(transport, app, config)` API.

```rust,no_run
use modbus_rs::{
  ClientServices, MAX_ADU_FRAME_LEN, MbusError, ModbusConfig, ModbusTcpConfig,
  RequestErrorNotifier, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};
use modbus_rs::heapless::Vec;

#[cfg(feature = "coils")]
use modbus_rs::{CoilResponse, Coils};

struct MockTransport;

impl Transport for MockTransport {
    type Error = MbusError;

    fn connect(&mut self, _: &ModbusConfig) -> Result<(), Self::Error> { Ok(()) }
    fn disconnect(&mut self) -> Result<(), Self::Error> { Ok(()) }
    fn send(&mut self, _: &[u8]) -> Result<(), Self::Error> { Ok(()) }
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> { Ok(Vec::new()) }
    fn is_connected(&self) -> bool { true }
    fn transport_type(&self) -> TransportType { TransportType::StdTcp }
}

struct App;

impl RequestErrorNotifier for App {
    fn request_failed(&self, _: u16, _: UnitIdOrSlaveAddr, _: MbusError) {}
}

#[cfg(feature = "coils")]
impl CoilResponse for App {
    fn read_coils_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: &Coils) {}
    fn read_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl TimeKeeper for App {
    fn current_millis(&self) -> u64 { 0 }
}

fn main() -> Result<(), MbusError> {
    let transport = MockTransport;
    let app = App;
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);

    let mut client = ClientServices::<_, _, 4>::new(transport, app, config)?;

    #[cfg(feature = "coils")]
    client.coils().read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;

    client.poll();
    Ok(())
}
```

  ### Feature-Scoped Request Access

  Request APIs can be accessed via feature facades:

  - `client.coils()`
  - `client.registers()`
  - `client.discrete_inputs()`
  - `client.diagnostic()`
  - `client.fifo()`
  - `client.file_records()`

  Batch request submission with scoped borrows:

  ```rust
  client.with_coils(|coils| {
    coils.read_single_coil(10, UnitIdOrSlaveAddr::new(1)?, 0)?;
    coils.write_single_coil(11, UnitIdOrSlaveAddr::new(1)?, 0, true)?;
    Ok::<(), MbusError>(())
  })?;
  ```

  ### Connection Recovery

  `ClientServices` supports explicit reconnection:

  - `client.is_connected()` checks transport state.
  - `client.reconnect()` reconnects with existing config.

  On reconnect, pending in-flight requests are failed with `MbusError::ConnectionLost`
  and removed from the queue. Applications should requeue requests explicitly after
  reconnection succeeds.

  ### Serial Compile-Time Queue Safety

  For serial RTU/ASCII clients, only one request will be in flight.

  - `ClientServices::new(...)` enforces this at runtime.
  - `ClientServices::new_serial(...)` enforces this at compile time.
  - Use `SerialClientServices<TRANSPORT, APP>` for readability.

## 3. Example Programs

The workspace contains real examples in `modbus-rs/examples/`.

### TCP examples

- [coils_example.rs](../modbus-rs/examples/coils_example.rs)
- [registers_example.rs](../modbus-rs/examples/registers_example.rs)
- [discrete_inputs_example.rs](../modbus-rs/examples/discrete_inputs_example.rs)
- [device_id_example.rs](../modbus-rs/examples/device_id_example.rs)

### Serial RTU examples

- [coils_serial_example.rs](../modbus-rs/examples/coils_serial_example.rs)
- [registers_serial_example.rs](../modbus-rs/examples/registers_serial_example.rs)
- [discrete_inputs_serial_example.rs](../modbus-rs/examples/discrete_inputs_serial_example.rs)
- [device_id_serial_example.rs](../modbus-rs/examples/device_id_serial_example.rs)

### Serial ASCII examples

- [ascii_serial_example.rs](../modbus-rs/examples/ascii_serial_example.rs)

### Async TCP examples

- [async_tcp_example.rs](../modbus-rs/examples/async_tcp_example.rs)

### Async Serial RTU examples

- [async_serial_rtu_example.rs](../modbus-rs/examples/async_serial_rtu_example.rs)

## 4. Async Usage (Tokio)

The `async` feature enables `mbus-async`, which provides `AsyncTcpClient` and
`AsyncSerialClient` with full `.await` support. No changes to application code structure
are required beyond adding `async`/`.await`.

`AsyncTcpClient` uses a compile-time pipeline depth const generic:

- default (`N = 9`): `AsyncTcpClient::connect(...)`
- custom: `AsyncTcpClient::<N>::connect_with_pipeline(...)`

Example with a custom compile-time pipeline depth:

```rust,no_run
use modbus_rs::mbus_async::AsyncTcpClient;

let client = AsyncTcpClient::<16>::connect_with_pipeline("192.168.1.10", 502)?;
```

Add the dependency:

```toml
[dependencies]
modbus-rs = { version = "0.4.0", features = ["async"] }
tokio = { version = "1", features = ["full"] }
```

### Async TCP example

```rust,no_run
use modbus_rs::mbus_async::AsyncTcpClient;
use modbus_rs::Coils;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AsyncTcpClient::connect("192.168.1.10", 502)?;

    // Read
    let coils = client.read_multiple_coils(1, 0, 8).await?;
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("coil[{}] = {}", addr, coils.value(addr)?);
    }

    let regs = client.read_holding_registers(1, 0, 4).await?;
    for addr in regs.from_address()..regs.from_address() + regs.quantity() {
        println!("reg[{}] = {}", addr, regs.value(addr)?);
    }

    // Write
    let (addr, val) = client.write_single_coil(1, 0, true).await?;
    println!("Wrote coil[{}] = {}", addr, val);

    let (start, qty) = client.write_multiple_registers(1, 0, &[100, 200, 300, 400]).await?;
    println!("Wrote {} registers starting at {}", qty, start);

    // Combined read/write
    let rw = client.read_write_multiple_registers(1, 0, 4, 10, &[1, 2]).await?;
    println!("Read {} registers starting at {}", rw.quantity(), rw.from_address());

    // Mask write register
    client.mask_write_register(1, 0, 0xFF00, 0x0055).await?;

    Ok(())
}
```

### Async serial RTU example

```rust,no_run
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

    let client = AsyncSerialClient::connect_rtu(config)?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("coil[{}] = {}", addr, coils.value(addr)?);
    }

    Ok(())
}
```

### Async optional poll interval

Both TCP and serial constructors have a `_with_poll_interval` variant that lets you tune
how often the worker thread checks for responses. The default is 20ms.

```rust,no_run
use modbus_rs::mbus_async::AsyncTcpClient;
use std::time::Duration;

let client = AsyncTcpClient::connect_with_poll_interval("192.168.1.10", 502, Duration::from_millis(5))?;
```

A lower value reduces latency at the cost of more CPU. A higher value is fine if response
times are expected to be hundreds of milliseconds.

## 5. Running Examples

Run examples from the workspace root.

### TCP sync example

```bash
cargo run -p modbus-rs --example coils_example --no-default-features --features client,tcp,coils
```

### Serial RTU sync example

```bash
cargo run -p modbus-rs --example coils_serial_example --no-default-features --features client,serial-rtu,coils
```

### Serial ASCII sync example

```bash
cargo run -p modbus-rs --example ascii_serial_example --no-default-features --features client,serial-ascii,coils
```

### Async TCP example

```bash
cargo run -p modbus-rs --example async_tcp_example --features async
# With explicit host/port/unit:
cargo run -p modbus-rs --example async_tcp_example --features async -- 192.168.1.10 502 1
```

### Async serial RTU example

```bash
cargo run -p modbus-rs --example async_serial_rtu_example \
  --no-default-features --features async,serial-rtu,coils,registers
# With explicit port/unit:
cargo run -p modbus-rs --example async_serial_rtu_example \
  --no-default-features --features async,serial-rtu,coils,registers \
  -- /dev/ttyUSB0 1
```

You can also use the default feature set if you want everything enabled:

```bash
cargo run -p modbus-rs --example coils_example
```

## 6. Transport Setup Notes

### TCP

For TCP examples, point the client at a reachable Modbus TCP server.

Common options:
- ModbusPal
- Simply Modbus TCP Slave
- a small `pymodbus` test server

### Serial RTU

For RTU examples, make sure the client and server agree on:
- baud rate
- parity
- data bits
- stop bits
- slave address

The RTU examples in this repository typically use:
- `SerialMode::Rtu`
- `8` data bits
- `1` stop bit
- `Parity::None`

### Serial ASCII

ASCII uses different framing rules than RTU.

The ASCII example in this repository uses:
- `SerialMode::Ascii`
- `7` data bits
- `1` stop bit
- `Parity::Even`

Use `serial-ascii` when compiling top-level `modbus-rs` builds intended for ASCII mode.

## 6. When to Use Helper Crates Directly

Use `modbus-rs` if you want a single dependency.

Use helper crates directly when you need lower-level control:
- `mbus-core` for shared protocol types and transport abstractions
- `mbus-client` for direct client orchestration access
- `mbus-network` for direct TCP transport usage
- `mbus-serial` for direct RTU/ASCII transport usage