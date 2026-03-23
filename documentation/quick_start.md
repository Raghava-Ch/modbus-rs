# Modbus-rs Quick Start Guide

This guide shows the current way to start with `modbus-rs` using the top-level crate and
its feature flags.

For detailed feature combinations, see [feature_flags.md](feature_flags.md).

## 1. Choose Your Transport and Features

`modbus-rs` is the recommended top-level dependency for most users.

### Full default stack

```toml
[dependencies]
modbus-rs = "0.1.0"
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
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

### Minimal Serial RTU client

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "serial-rtu",
  "registers"
] }
```

### Minimal Serial ASCII client

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "serial-ascii",
  "coils"
] }
```

## 2. Basic Usage Example (TCP)

This example uses the current `ClientServices::new(transport, app, config)` API.

```rust,no_run
use modbus_rs::errors::MbusError;
use modbus_rs::transport::{
    ModbusConfig, ModbusTcpConfig, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};
use modbus_rs::modbus_client::app::RequestErrorNotifier;
use modbus_rs::modbus_client::services::ClientServices;

use modbus_rs::data_unit::common::MAX_ADU_FRAME_LEN;
use modbus_rs::heapless::Vec;

#[cfg(feature = "coils")]
use modbus_rs::modbus_client::app::CoilResponse;
#[cfg(feature = "coils")]
use modbus_rs::modbus_client::services::coil::Coils;

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
    client.read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;

    client.poll();
    Ok(())
}
```

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

## 4. Running Examples

Run examples from the workspace root.

### TCP example

```bash
cargo run -p modbus-rs --example coils_example --no-default-features --features client,tcp,coils
```

### Serial RTU example

```bash
cargo run -p modbus-rs --example coils_serial_example --no-default-features --features client,serial-rtu,coils
```

### Serial ASCII example

```bash
cargo run -p modbus-rs --example ascii_serial_example --no-default-features --features client,serial-ascii,coils
```

You can also use the default feature set if you want everything enabled:

```bash
cargo run -p modbus-rs --example coils_example
```

## 5. Transport Setup Notes

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
- `modbus-client` for direct client orchestration access
- `mbus-tcp` for direct TCP transport usage
- `mbus-serial` for direct RTU/ASCII transport usage