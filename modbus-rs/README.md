# modbus-rs

`modbus-rs` is the top-level crate in the workspace and the recommended entry point for
most users.

It re-exports the core protocol crate, client services, and optional TCP/Serial transport
implementations behind feature flags so you can choose between convenience and minimal
binary size.

## What This Crate Is

`modbus-rs` is a convenience crate.

It brings together:

- `mbus-core` for shared protocol types and transport abstractions
- `modbus-client` for client-side request/response orchestration
- `mbus-tcp` for standard TCP transport
- `mbus-serial` for standard Serial RTU/ASCII transport

If you want a single dependency for most applications, use `modbus-rs`.
If you need lower-level control, you can depend on the helper crates directly.

## What Is Included

Depending on enabled features, this crate re-exports:

- all public items from `mbus-core`
- all public items from `mbus-tcp`
- all public items from `mbus-serial`
- the `modbus_client` crate
- `heapless`

## Feature Flags

Top-level features:

- `client`: enables `modbus-client`
- `serial-rtu`: enables `mbus-serial` for RTU transport use cases
- `serial-ascii`: enables `mbus-serial` for ASCII transport use cases
- `tcp`: enables `mbus-tcp`
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`

Default behavior:

- `default` enables `client`, `serial-rtu`, `serial-ascii`, `tcp`, and all function-group features.

Example: only enable client + TCP + coil support:

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

For more feature combinations, see [documentation/feature_flags.md](../documentation/feature_flags.md).

## Quick Start

### Default full setup

```toml
[dependencies]
modbus-rs = "0.1.0"
```

### Minimal TCP client setup

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "tcp",
  "registers"
] }
```

## Basic Usage Example

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

## Examples

### TCP examples

- [coils_example.rs](examples/coils_example.rs)
- [registers_example.rs](examples/registers_example.rs)
- [discrete_inputs_example.rs](examples/discrete_inputs_example.rs)
- [device_id_example.rs](examples/device_id_example.rs)

### Serial RTU examples

- [coils_serial_example.rs](examples/coils_serial_example.rs)
- [registers_serial_example.rs](examples/registers_serial_example.rs)
- [discrete_inputs_serial_example.rs](examples/discrete_inputs_serial_example.rs)
- [device_id_serial_example.rs](examples/device_id_serial_example.rs)

### Serial ASCII examples

- [ascii_serial_example.rs](examples/ascii_serial_example.rs)

Run examples from the workspace root:

```bash
# TCP
cargo run -p modbus-rs --example coils_example --no-default-features --features client,tcp,coils

# Serial RTU
cargo run -p modbus-rs --example coils_serial_example --no-default-features --features client,serial-rtu,coils

# Serial ASCII
cargo run -p modbus-rs --example ascii_serial_example --no-default-features --features client,serial-ascii,coils
```

## Workspace Structure

- `modbus-rs`: top-level convenience crate
- `mbus-core`: shared protocol and transport abstractions
- `modbus-client`: client state machine and service modules
- `mbus-tcp`: standard TCP transport helper crate
- `mbus-serial`: standard serial transport helper crate

## Documentation

Additional workspace documentation is available in the `documentation/` folder:

- [documentation/quick_start.md](../documentation/quick_start.md)
- [documentation/architecture.md](../documentation/architecture.md)
- [documentation/feature_flags.md](../documentation/feature_flags.md)

## Notes

- This project is under active development.
- Feature names use hyphenated forms such as `discrete-inputs` and `file-record`.
- A future server-side feature is planned but not implemented yet.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](./LICENSE) for details.

## Disclaimer

This is an independent Rust implementation of the Modbus specification and is not
affiliated with the Modbus Organization.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)