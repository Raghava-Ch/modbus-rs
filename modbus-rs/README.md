# modbus-rs

`modbus-rs` is a low-footprint, cross-platform Modbus client workspace built for both embedded and desktop/server systems.
It runs on no_std and std targets (MCUs, RTOS, Windows, Linux, macOS), supports TCP/RTU/ASCII, provides sync and async APIs, and uses feature gating to keep binaries minimal.
Advanced reliability features include configurable retry, backoff, and jitter, with optional native C and WASM bindings via `mbus-ffi`.

It re-exports the core protocol crate, client services, and optional TCP/Serial transport
implementations behind feature flags so you can choose between convenience and minimal
binary size.

## Basic Async Usage Example

```rust,no_run
use modbus_rs::Coils;
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> Result<()> {
	let host = "127.0.0.1";
    let port = 502;
    let unit_id = 1;

    let client = AsyncTcpClient::new(&host, port)?;
    client.connect().await?;

    let coils: Coils = client.read_multiple_coils(unit_id, 0, 8).await?;
	for addr in 0..8 {
		println!("coil[{}] = {}", addr, coils.value(addr)?);
	}

    let (wr_addr, wr_val) = client.write_single_coil(unit_id, 0, true).await?;
    println!("Wrote coil[{}] = {}", wr_addr, wr_val);
}
```

## What This Crate Is

`modbus-rs` is a convenience crate.

It brings together:

- `mbus-core` for shared protocol types and transport abstractions
- `mbus-client` for client-side request/response orchestration
- `mbus-network` for standard TCP transport
- `mbus-serial` for standard Serial RTU/ASCII transport

If you want a single dependency for most applications, use `modbus-rs`.
If you need lower-level control, you can depend on the helper crates directly.

## Public Entry Point Policy

For consumers, `modbus-rs` is the intended public API surface.

- Use `modbus-rs` in application `Cargo.toml`.
- Access all request/response service features through `modbus-rs` re-exports.
- For browser/WASM or native C integrations, use `mbus-ffi` directly.

Helper crates (`mbus-core`, `mbus-client`, `mbus-network`, `mbus-serial`, `mbus-ffi`) remain workspace building blocks.

## What Is Included

Depending on enabled features, this crate re-exports:

- all public items from `mbus-core`
- all public items from `mbus-network`
- all public items from `mbus-serial`
- the `mbus_client` crate
- `heapless`

## Feature Flags

Top-level features:

- `client`: enables `mbus-client`
- `serial-rtu`: enables `mbus-serial` for RTU transport use cases
- `serial-ascii`: enables `mbus-serial` for ASCII transport use cases
- `tcp`: enables `mbus-network`
- `async`: enables `mbus-async` async facade re-export (`modbus_rs::mbus_async`)
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `traffic`: enables raw TX/RX frame observability hooks for sync and async clients
- `logging`: enables `log` facade diagnostics in `mbus-network` and `mbus-serial`

Default behavior:

- `default` enables `client`, `serial-rtu`, `tcp`, and all function-group features.
- `serial-ascii` and `async` are opt-in.
- `serial-ascii`, `async`, and `traffic` are opt-in.
- `async` is opt-in and must be enabled explicitly when using `.await` APIs.

Example: only enable client + TCP + coil support:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

For more feature combinations, see [documentation/feature_flags.md](../documentation/feature_flags.md).

### Async Setup

Enable async APIs with the `async` feature and add Tokio:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
	"async",
	"tcp",
	"coils"
] }
tokio = { version = "1", features = ["full"] }
```

Use async clients from `modbus_rs::mbus_async` and connect explicitly before requests:

```rust,no_run
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
		let client = AsyncTcpClient::new("127.0.0.1", 502)?;
		client.connect().await?;

		let _coils = client.read_multiple_coils(1, 0, 8).await?;
		Ok(())
}
```

### Traffic Hooks Setup

Enable traffic observability for raw ADU TX/RX frame callbacks:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
	"client",
	"tcp",
	"coils",
	"traffic"
] }
```

Dedicated examples:

- sync traffic observer: `examples/traffic_sync_example.rs`
- async traffic observer: `examples/traffic_async_tcp_example.rs`

### Logging Setup

The `logging` feature enables instrumentation points through the `log` facade.
To see output, initialize a logger backend in your application (for example `env_logger`).

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = ["tcp", "logging"] }
env_logger = "0.11"
```

## Quick Start

### Default full setup

```toml
[dependencies]
modbus-rs = "0.5.0"
```

### Minimal TCP client setup

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "tcp",
  "registers"
] }
```

### WASM browser setup (independent `mbus-ffi` crate)

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = ["client", "tcp", "coils"] }
```

Then use `mbus-ffi` for browser/WASM bindings:

```bash
cd /path/to/modbus-rs
wasm-pack build ./mbus-ffi --target web --features wasm,full
cd mbus-ffi
python3 -m http.server 8089
```

## Bindings

Bindings are implemented in the `mbus-ffi` crate and distributed separately from the top-level `modbus-rs` Rust API.

- WASM/browser bindings:
	- crate docs and usage: [../mbus-ffi/README.md](../mbus-ffi/README.md)
	- browser smoke pages: `mbus-ffi/examples/network_smoke.html` and `mbus-ffi/examples/serial_smoke.html`
- Native C bindings:
	- C header: `mbus-ffi/include/mbus_ffi.h`
	- C smoke example: `mbus-ffi/examples/c_smoke_cmake/`
	- C test instructions: [../mbus-ffi/README.md](../mbus-ffi/README.md)

If you are building browser or native C integrations, start from `mbus-ffi` directly.

For browser smoke pages in this workspace, build and serve the `mbus-ffi` package path (implementation package used by the HTML examples):

```bash
cd /path/to/modbus-rs
wasm-pack build ./mbus-ffi --target web --features wasm,full
cd mbus-ffi
python3 -m http.server 8089
```

## Basic Usage Example

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
	client.connect()?;

	#[cfg(feature = "coils")]
	client.coils().read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;

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
- [feature_facades_showcase.rs](examples/feature_facades_showcase.rs)
- [tcp_backoff_jitter_example.rs](examples/tcp_backoff_jitter_example.rs)
- [logging_example.rs](examples/logging_example.rs)
- [traffic_sync_example.rs](examples/traffic_sync_example.rs) (`traffic` feature)
- [traffic_async_tcp_example.rs](examples/traffic_async_tcp_example.rs) (`async,traffic` features)

### Serial RTU examples

- [coils_serial_example.rs](examples/coils_serial_example.rs)
- [registers_serial_example.rs](examples/registers_serial_example.rs)
- [discrete_inputs_serial_example.rs](examples/discrete_inputs_serial_example.rs)
- [device_id_serial_example.rs](examples/device_id_serial_example.rs)
- [serial_rtu_backoff_jitter_example.rs](examples/serial_rtu_backoff_jitter_example.rs)

### Serial ASCII examples

- [ascii_serial_example.rs](examples/ascii_serial_example.rs)

Run examples from the workspace root:

```bash
# TCP
cargo run -p modbus-rs --example coils_example --no-default-features --features client,tcp,coils
cargo run -p modbus-rs --example registers_example --no-default-features --features client,tcp,registers
cargo run -p modbus-rs --example discrete_inputs_example --no-default-features --features client,tcp,discrete-inputs
cargo run -p modbus-rs --example device_id_example --no-default-features --features client,tcp,diagnostics
cargo run -p modbus-rs --example feature_facades_showcase --no-default-features --features client,tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
cargo run -p modbus-rs --example tcp_backoff_jitter_example --no-default-features --features client,tcp,coils
cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,logging
cargo run -p modbus-rs --example traffic_sync_example --no-default-features --features client,tcp,coils,traffic

# Async
cargo run -p modbus-rs --example async_tcp_example --no-default-features --features async,tcp,coils,registers,discrete-inputs
cargo run -p modbus-rs --example traffic_async_tcp_example --no-default-features --features async,tcp,coils,traffic

# Serial RTU
cargo run -p modbus-rs --example coils_serial_example --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example registers_serial_example --no-default-features --features client,serial-rtu,registers
cargo run -p modbus-rs --example discrete_inputs_serial_example --no-default-features --features client,serial-rtu,discrete-inputs
cargo run -p modbus-rs --example device_id_serial_example --no-default-features --features client,serial-rtu,diagnostics
cargo run -p modbus-rs --example serial_rtu_backoff_jitter_example --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example async_serial_rtu_example --no-default-features --features async,serial-rtu,coils,registers

# Serial ASCII
cargo run -p modbus-rs --example ascii_serial_example --no-default-features --features client,serial-ascii,coils
```

## Workspace Structure

- `modbus-rs`: top-level convenience crate
- `mbus-core`: shared protocol and transport abstractions
- `mbus-client`: client state machine and service modules
- `mbus-network`: standard TCP transport helper crate
- `mbus-serial`: standard serial transport helper crate

## Browser Smoke Examples

Browser examples currently live under `mbus-ffi/examples` and are intended for quick WASM smoke validation.

- `network_smoke.html`
- `serial_smoke.html`

After building `mbus-ffi/pkg`, open:

- `http://localhost:8089/examples/network_smoke.html`
- `http://localhost:8089/examples/serial_smoke.html`

Run WASM browser feature tests:

```bash
cd mbus-ffi;
wasm-pack test --chrome --target wasm32-unknown-unknown --features wasm,full
```

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