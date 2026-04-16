# modbus-rs

`modbus-rs` is a low-footprint, cross-platform Modbus client workspace built for both embedded and desktop/server systems.
It runs on no_std and std targets (MCUs, RTOS, Windows, Linux, macOS), supports TCP/RTU/ASCII, provides sync and async APIs, and uses feature gating to keep binaries minimal.
Advanced reliability features include configurable retry, backoff, and jitter, with optional native C and WASM bindings via `mbus-ffi`.

It re-exports the core protocol crate, client services, and optional TCP/Serial transport
implementations behind feature flags so you can choose between convenience and minimal
binary size.

## Basic Async Usage Example

```rust,no_run
use anyhow::Result;
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

	Ok(())
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

### std vs no_std

The default feature set is **std-friendly** — it enables transports and the client state machine out of the box for desktop/server use.

For **embedded / no_std** targets, disable defaults and use the `no-std` convenience feature:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = ["no-std"] }
```

This enables the `mbus-client` state machine and all function code models (`coils`, `registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`) without pulling in any transport. Provide your own `Transport` implementation for your hardware.

Features that **require std**: `tcp`, `serial-rtu`, `serial-ascii`, `async`, `logging`.  
Features that are **no_std compatible**: `client`, `coils`, `registers`, `holding-registers`, `input-registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`, `traffic`.

---

### Top-level features

- `client`: enables `mbus-client`
- `server`: enables `mbus-server` re-exports including `ServerServices`,
  resilience configuration, and derive-based server helpers
- `serial-rtu`: enables `mbus-serial` for RTU transport use cases _(requires std)_
- `serial-ascii`: enables `mbus-serial` for ASCII transport use cases _(requires std)_
- `tcp`: enables `mbus-network` _(requires std)_
- `async`: enables `mbus-async` async facade re-export (`modbus_rs::mbus_async`) _(requires std)_
- `coils`
- `registers`
- `holding-registers` (alias of `registers`; useful when matching server-side naming)
- `input-registers` (alias of `registers`; useful when matching server-side naming)
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `traffic`: enables raw TX/RX frame observability hooks for sync and async clients
- `logging`: enables `log` facade diagnostics in `mbus-network` and `mbus-serial` _(requires std)_
- `no-std`: convenience bundle — `client` + all FC models, no transports

Default behavior:

- `default` enables `client`, `serial-rtu`, `tcp`, and all function-group features.
- `server`, `serial-ascii`, `async`, and `traffic` are opt-in.
- `no-std` is opt-in; use it with `default-features = false` on embedded targets.

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

- sync traffic observer: `examples/modbus-rs/client/traffic/traffic_sync_example.rs`
- async traffic observer: `examples/modbus-rs/client/traffic/traffic_async_tcp_example.rs`

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
	const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;
	const SUPPORTS_BROADCAST_WRITES: bool = false;

	fn connect(&mut self, _: &ModbusConfig) -> Result<(), Self::Error> { Ok(()) }
	fn disconnect(&mut self) -> Result<(), Self::Error> { Ok(()) }
	fn send(&mut self, _: &[u8]) -> Result<(), Self::Error> { Ok(()) }
	fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> { Ok(Vec::new()) }
	fn is_connected(&self) -> bool { true }
}

struct App;

impl RequestErrorNotifier for App {
	fn request_failed(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: MbusError) {}
}

#[cfg(feature = "coils")]
impl CoilResponse for App {
	fn read_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &Coils) {}
	fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
	fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
	fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl TimeKeeper for App {
	fn current_millis(&self) -> u64 { 0 }
}

#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for App {}

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

- [coils_example.rs](examples/modbus-rs/client/tcp/coils_example.rs)
- [registers_example.rs](examples/modbus-rs/client/tcp/registers_example.rs)
- [discrete_inputs_example.rs](examples/modbus-rs/client/tcp/discrete_inputs_example.rs)
- [device_id_example.rs](examples/modbus-rs/client/tcp/device_id_example.rs)
- [feature_facades_showcase.rs](examples/modbus-rs/client/showcase/feature_facades_showcase.rs)
- [tcp_backoff_jitter_example.rs](examples/modbus-rs/client/tcp/tcp_backoff_jitter_example.rs)
- [logging_example.rs](examples/modbus-rs/client/tcp/logging_example.rs)
- [traffic_sync_example.rs](examples/modbus-rs/client/traffic/traffic_sync_example.rs) (`traffic` feature)
- [traffic_async_tcp_example.rs](examples/modbus-rs/client/traffic/traffic_async_tcp_example.rs) (`async,traffic` features)

### Serial RTU examples

- [coils_serial_example.rs](examples/modbus-rs/client/serial/coils_serial_example.rs)
- [registers_serial_example.rs](examples/modbus-rs/client/serial/registers_serial_example.rs)
- [discrete_inputs_serial_example.rs](examples/modbus-rs/client/serial/discrete_inputs_serial_example.rs)
- [device_id_serial_example.rs](examples/modbus-rs/client/serial/device_id_serial_example.rs)
- [serial_rtu_backoff_jitter_example.rs](examples/modbus-rs/client/serial/serial_rtu_backoff_jitter_example.rs)

### Serial ASCII examples

- [ascii_serial_example.rs](examples/modbus-rs/client/serial/ascii_serial_example.rs)

Run examples from the workspace root:

```bash
# TCP
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils --no-default-features --features client,tcp,coils
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers --no-default-features --features client,tcp,registers
cargo run -p modbus-rs --example modbus_rs_client_tcp_discrete_inputs --no-default-features --features client,tcp,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_tcp_device_id --no-default-features --features client,tcp,diagnostics
# Source-only showcase example (not currently exposed as a Cargo example target):
# cargo run -p modbus-rs --example modbus_rs_client_showcase_feature_facades --no-default-features --features client,tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter --no-default-features --features client,tcp,coils
cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,logging
cargo run -p modbus-rs --example modbus_rs_client_traffic_sync_tcp --no-default-features --features client,tcp,coils,traffic

# Async
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --no-default-features --features async,client,tcp,coils,registers,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_traffic_async_tcp --no-default-features --features async,tcp,coils,traffic

# Serial RTU
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_coils --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers --no-default-features --features client,serial-rtu,registers
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_discrete_inputs --no-default-features --features client,serial-rtu,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_device_id --no-default-features --features client,serial-rtu,diagnostics
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_backoff_jitter --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example modbus_rs_client_async_serial_rtu --no-default-features --features async,serial-rtu,coils,registers

# Serial ASCII
cargo run -p modbus-rs --example modbus_rs_client_serial_ascii_coils --no-default-features --features client,serial-ascii,coils
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
- Server APIs are available behind the `server` feature.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](../LICENSE) for details.

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

## Disclaimer

This is an independent Rust implementation of the Modbus specification and is not
affiliated with the Modbus Organization.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)