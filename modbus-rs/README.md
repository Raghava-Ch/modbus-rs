# modbus-rs

`modbus-rs` is a low-footprint, cross-platform Modbus client workspace built for both embedded and desktop/server systems.
It runs on no_std and std targets (MCUs, RTOS, Windows, Linux, macOS), supports TCP/RTU/ASCII, provides sync and async APIs, and uses feature gating to keep binaries minimal.
Advanced reliability features include configurable retry, backoff, and jitter, with optional native C and WASM bindings via `mbus-ffi`.
A built-in Modbus gateway bridges TCP upstream connections to RTU/ASCII or TCP downstream slaves, in both sync (no_std-compatible) and async (Tokio) modes.

It re-exports the core protocol crate, client and server services, and optional TCP/Serial transport
implementations behind feature flags so you can choose between convenience and minimal
binary size.

## Basic Async Client Usage Example

```rust
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

## Basic Async Server Usage Example

<!-- validate: no_run -->
```rust
use anyhow::Result;
use modbus_rs::mbus_async::server::AsyncTcpServer;
use modbus_rs::{async_modbus_app, CoilsModel, HoldingRegistersModel};
use modbus_rs::UnitIdOrSlaveAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)] enabled: bool,
    #[coil(addr = 1)] alarm: bool,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct Regs {
    #[reg(addr = 0)] setpoint: u16,
    #[reg(addr = 1)] feedback: u16,
}

#[derive(Default)]
#[async_modbus_app]
struct App {
    #[coils]
    coils: Coils,
    #[holding_registers]
    regs: Regs,
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = Arc::new(Mutex::new(App::default()));
    AsyncTcpServer::serve_shared("0.0.0.0:5502", app, UnitIdOrSlaveAddr::new(1)?).await?;
    Ok(())
}
```
## Documentation

Additional workspace documentation is available in the `documentation/` folder:

- [documentation/README.md](../documentation/README.md) — top-level navigation
- [documentation/migration_guide.md](../documentation/migration_guide.md) — version-to-version migration notes

### Client docs

- [documentation/client/README.md](../documentation/client/README.md)
- [documentation/client/quick_start.md](../documentation/client/quick_start.md)

### Server docs

- [documentation/server/README.md](../documentation/server/README.md)
- [documentation/server/quick_start.md](../documentation/server/quick_start.md)

### Gateway docs

- [documentation/gateway/quick_start.md](../documentation/gateway/quick_start.md)
- [documentation/gateway/architecture.md](../documentation/gateway/architecture.md)
- [documentation/gateway/routing.md](../documentation/gateway/routing.md)
- [documentation/gateway/ws_gateway.md](../documentation/gateway/ws_gateway.md)
- [documentation/gateway/feature_flags.md](../documentation/gateway/feature_flags.md)


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
- `ClientServices`, `SerialClientServices` from `mbus-client`
- `ServerServices`, `ResilienceConfig`, `ForwardingApp`, `modbus_app`, `async_modbus_app`, model derive traits, and all FC handler traits from `mbus-server` (behind `server` feature)
- `mbus_async` module re-export (behind `async` feature)
- `heapless`

## Feature Flags

### std vs no_std

The default feature set is **std-friendly** — it enables transports and the client state machine out of the box for desktop/server use.

For **embedded / no_std** targets, disable defaults and use the `no-std` convenience feature:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = ["no-std"] }
```

This enables the `mbus-client` state machine and all function code models (`coils`, `registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`) without pulling in any transport. Provide your own `Transport` implementation for your hardware.

Features that **require std**: `network-tcp`, `serial-rtu`, `serial-ascii`, `async`, `logging`.  
Features that are **no_std compatible**: `client`, `coils`, `registers`, `holding-registers`, `input-registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`, `traffic`.

---

### Top-level features

- `client`: enables `mbus-client`
- `server`: enables `mbus-server` re-exports including `ServerServices`,
  resilience configuration, and derive-based server helpers
- `serial-rtu`: enables `mbus-serial` for RTU transport use cases _(requires std)_
- `serial-ascii`: enables `mbus-serial` for ASCII transport use cases _(requires std)_
- `network-tcp`: enables `mbus-network` _(requires std)_
- `async`: enables native async runtime and APIs via Tokio (`modbus_rs::mbus_async`) _(requires std)_
- `async` + `network-tcp`: enables async TCP client and server paths
- `async` + `serial-rtu`/`serial-ascii`: enables async serial client and server paths
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
- `gateway`: enables `mbus-gateway` — Modbus TCP ↔ RTU/ASCII/TCP gateway (sync + async)
- `no-std`: convenience bundle — `client` + all FC models, no transports

Default behavior:

- `default` enables `client`, `server`, `serial-rtu`, `network-tcp`, and all function-group features.
- `serial-ascii`, `async`, and `traffic` are opt-in.
- `no-std` is opt-in; use it with `default-features = false` on embedded targets.

Example: only enable sync client + TCP + coil support:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = [
  "client",
	"network-tcp",
  "coils"
] }
```

For more feature combinations:
- Server: [documentation/server/feature_flags.md](../documentation/server/feature_flags.md).
- Client: [documentation/client/feature_flags.md](../documentation/client/feature_flags.md).
- Gateway: [documentation/gateway/feature_flags.md](../documentation/gateway/feature_flags.md).

### Server Setup

Enable the server runtime with the `server` feature. Sync servers also need a transport feature (`network-tcp`, `serial-rtu`, or `serial-ascii`). Async servers use `async` plus a transport feature.

```toml
[dependencies]
# Sync TCP server
modbus-rs = { version = "0.8.0", features = ["server", "network-tcp", "coils", "holding-registers"] }

# Async TCP server
modbus-rs = { version = "0.8.0", features = ["server", "async", "network-tcp", "coils", "holding-registers"] }
tokio = { version = "1", features = ["full"] }
```

Using `#[modbus_app]` derive macro (sync server):

<!-- validate: no_run -->
```rust
use modbus_rs::{
    ServerServices, ResilienceConfig, StdTcpServerTransport,
    modbus_app, ServerCoilHandler, ServerHoldingRegisterHandler,
    ServerExceptionHandler, CoilsModel, HoldingRegistersModel,
};
use modbus_rs::{ModbusConfig, ModbusTcpConfig, UnitIdOrSlaveAddr};

#[derive(Debug, Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)] run_enable: bool,
    #[coil(addr = 1)] pump_enable: bool,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct Regs {
    #[reg(addr = 0)] setpoint: u16,
    #[reg(addr = 1)] mode: u16,
}

#[modbus_app]
struct App {
	#[coils]
	coils: Coils,
	#[holding_registers]
	regs: Regs,
}

// `#[modbus_app]` generates the required server handler impls.
// Then pass App to ServerServices with a StdTcpServerTransport per accepted connection.
```

Using `#[async_modbus_app]` (async TCP server with shared state):

<!-- validate: no_run -->
```rust
use modbus_rs::mbus_async::server::AsyncTcpServer;
use modbus_rs::{async_modbus_app, CoilsModel, HoldingRegistersModel};
use modbus_rs::UnitIdOrSlaveAddr;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Default, CoilsModel)]
struct Coils { #[coil(addr = 0)] enabled: bool }

#[derive(Debug, Default, HoldingRegistersModel)]
struct Regs { #[reg(addr = 0)] setpoint: u16 }

#[derive(Default)]
#[async_modbus_app]
struct App {
	#[coils]
	coils: Coils,
	#[holding_registers]
	regs: Regs,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let app = Arc::new(Mutex::new(App::default()));
    AsyncTcpServer::serve_shared("0.0.0.0:5502", app, UnitIdOrSlaveAddr::new(1)?).await?;
    Ok(())
}
```

### Gateway Setup

Enable the gateway with the `gateway` feature. Choose transport features to match your upstream/downstream protocol.

```toml
[dependencies]
# Sync gateway: TCP upstream → RTU downstream
modbus-rs = { version = "0.8.0", features = ["gateway", "network-tcp", "serial-rtu"] }

# Async gateway: TCP upstream → TCP downstream
modbus-rs = { version = "0.8.0", features = ["gateway", "async", "network-tcp"] }
tokio = { version = "1", features = ["full"] }
```

Sync poll-driven gateway (no_std-compatible):

<!-- validate: no_run -->
```rust
use modbus_rs::gateway::{GatewayServices, UnitRouteTable, NoopEventHandler, DownstreamChannel};

// Route unit IDs 1–10 to channel 0 (RTU downstream)
let mut routes = UnitRouteTable::new();
routes.add(1, 10, 0).unwrap();

let mut gw = GatewayServices::<_, _, 1, 8>::new(
    upstream_transport,
    [(downstream_rtu_transport, 0)],
    routes,
    NoopEventHandler,
);

loop { gw.poll(); }
```

See [documentation/gateway/quick_start.md](../documentation/gateway/quick_start.md) for async TCP and WebSocket gateway setup.

---

### Async Client Setup

Enable async APIs with the `async` feature and add Tokio:

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = [
	"async",
	"network-tcp",
	"coils"
] }
tokio = { version = "1", features = ["full"] }
```

Use async clients from `modbus_rs::mbus_async` and connect explicitly before requests:

```rust
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
modbus-rs = { version = "0.8.0", default-features = false, features = [
	"client",
	"network-tcp",
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
modbus-rs = { version = "0.8.0", default-features = false, features = ["network-tcp", "logging"] }
env_logger = "0.11"
```

## Quick Start

### Default full setup

```toml
[dependencies]
modbus-rs = "0.8.0"
```

### Minimal TCP client setup

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = [
  "client",
  "network-tcp",
  "registers"
] }
```

### WASM browser setup (independent `mbus-ffi` crate)

Use `mbus-ffi` directly for browser/WASM. `modbus-rs` does not re-export the WASM bindings.

```toml
[dependencies]
mbus-ffi = { version = "0.8.0", default-features = false, features = ["wasm", "full"] }
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
	- generated headers: `target/mbus-ffi/include/`
	- C client smoke example: `mbus-ffi/examples/c_client_demo/`
	- C server — hand-written handlers: `mbus-ffi/examples/c_server_demo/`
	- C server — YAML-driven codegen: `mbus-ffi/examples/c_server_demo_yaml/`
	- C test instructions: [../mbus-ffi/README.md](../mbus-ffi/README.md)

If you are building browser or native C integrations, start from `mbus-ffi` directly.

For browser smoke pages in this workspace, build and serve the `mbus-ffi` package path (implementation package used by the HTML examples):

```bash
cd /path/to/modbus-rs
wasm-pack build ./mbus-ffi --target web --features wasm,full
cd mbus-ffi
python3 -m http.server 8089
```

## Basic Sync Client Usage Example With Custom Transport

```rust
use modbus_rs::{
	ClientServices, MAX_ADU_FRAME_LEN, MbusError, ModbusConfig, ModbusTcpConfig,
	RequestErrorNotifier, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};

use modbus_rs::heapless::Vec;

#[allow(unexpected_cfgs)]
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

#[allow(unexpected_cfgs)]
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

#[allow(unexpected_cfgs)]
#[cfg(feature = "traffic")]
impl modbus_rs::TrafficNotifier for App {}

fn main() -> Result<(), MbusError> {
	let transport = MockTransport;
	let app = App;
	let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);

	let mut client = ClientServices::<_, _, 4>::new(transport, app, config)?;
	client.connect()?;

	#[allow(unexpected_cfgs)]
#[cfg(feature = "coils")]
	client.coils().read_multiple_coils(1, UnitIdOrSlaveAddr::new(1)?, 0, 8)?;

	client.poll();
	Ok(())
}
```

## Examples

### Server examples — TCP

- [server/network-tcp/sync/demo.rs](examples/server/network-tcp/sync/demo.rs) — sync multi-client TCP server with `#[modbus_app]`
- [server/network-tcp/sync/shared_state.rs](examples/server/network-tcp/sync/shared_state.rs) — sync server + client in-process demo
- [server/network-tcp/sync/fifo_file_record_demo.rs](examples/server/network-tcp/sync/fifo_file_record_demo.rs) — sync FIFO/file-record routing via `fifo(...)` and `file_record(...)`
- [server/network-tcp/async/demo.rs](examples/server/network-tcp/async/demo.rs) — async TCP server with `#[async_modbus_app]` and shared state
- [server/network-tcp/async/traffic.rs](examples/server/network-tcp/async/traffic.rs) — async TCP server with traffic hooks
- [server/network-tcp/async/fifo_file_record_demo.rs](examples/server/network-tcp/async/fifo_file_record_demo.rs) — async FIFO/file-record routing with live background updates

### Server examples — Serial

- [server/serial-rtu/sync/demo.rs](examples/server/serial-rtu/sync/demo.rs) — sync RTU server
- [server/serial-rtu/sync/manual_app_no_macros.rs](examples/server/serial-rtu/sync/manual_app_no_macros.rs) — sync RTU server without derive macros
- [server/serial-ascii/sync/demo.rs](examples/server/serial-ascii/sync/demo.rs) — sync ASCII server

### TCP Client examples

- [coils.rs](examples/client/network-tcp/sync/coils.rs)
- [registers.rs](examples/client/network-tcp/sync/registers.rs)
- [discrete_inputs.rs](examples/client/network-tcp/sync/discrete_inputs.rs)
- [device_id.rs](examples/client/network-tcp/sync/device_id.rs)
- [backoff_jitter.rs](examples/client/network-tcp/sync/backoff_jitter.rs)
- [logging.rs](examples/client/network-tcp/sync/logging.rs)
- [traffic.rs](examples/client/network-tcp/sync/traffic.rs) (`traffic` feature)
- [tcp.rs](examples/client/network-tcp/async/tcp.rs) (`async` feature)
- [traffic.rs](examples/client/network-tcp/async/traffic.rs) (`async,traffic` features)

### Serial RTU examples

- [coils.rs](examples/client/serial-rtu/sync/coils.rs)
- [registers.rs](examples/client/serial-rtu/sync/registers.rs)
- [discrete_inputs.rs](examples/client/serial-rtu/sync/discrete_inputs.rs)
- [device_id.rs](examples/client/serial-rtu/sync/device_id.rs)
- [backoff_jitter.rs](examples/client/serial-rtu/sync/backoff_jitter.rs)

### Serial ASCII examples

- [coils.rs](examples/client/serial-ascii/sync/coils.rs)

### Gateway examples

- [sync_tcp_to_rtu.rs](examples/gateway/sync_tcp_to_rtu.rs) — poll-driven TCP → RTU gateway, no async runtime
- [async_tcp_to_tcp.rs](examples/gateway/async_tcp_to_tcp.rs) — async TCP → TCP gateway with unit-ID remapping

Run examples from the workspace root:

```bash
# Sync TCP server
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo --features server,network-tcp,coils,holding-registers,input-registers

# Async TCP server
cargo run -p modbus-rs --example modbus_rs_server_async_tcp_demo --features server,async,network-tcp,coils,holding-registers,input-registers

# Sync TCP FIFO/file-record server
cargo run -p modbus-rs --example fifo_file_record_demo --features server,network-tcp,fifo,file-record

# Async TCP FIFO/file-record server
cargo run -p modbus-rs --example modbus_rs_server_async_fifo_file_record_demo --features server,async,network-tcp,fifo,file-record

# Sync RTU server
cargo run -p modbus-rs --example modbus_rs_server_serial_rtu_demo --features server,serial-rtu,coils,holding-registers,input-registers

# Sync ASCII server
cargo run -p modbus-rs --example modbus_rs_server_serial_ascii_demo --features server,serial-ascii,coils,holding-registers,input-registers

# TCP client examples
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils --no-default-features --features client,network-tcp,coils
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers --no-default-features --features client,network-tcp,registers
cargo run -p modbus-rs --example modbus_rs_client_tcp_discrete_inputs --no-default-features --features client,network-tcp,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_tcp_device_id --no-default-features --features client,network-tcp,diagnostics
# Source-only showcase example (not currently exposed as a Cargo example target):
# cargo run -p modbus-rs --example modbus_rs_client_showcase_feature_facades --no-default-features --features client,network-tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter --no-default-features --features client,network-tcp,coils
cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features network-tcp,logging
cargo run -p modbus-rs --example modbus_rs_client_traffic_sync_tcp --no-default-features --features client,network-tcp,coils,traffic

# Async
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --no-default-features --features async,client,network-tcp,coils,registers,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_traffic_async_tcp --no-default-features --features async,network-tcp,coils,traffic

# Serial RTU
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_coils --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers --no-default-features --features client,serial-rtu,registers
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_discrete_inputs --no-default-features --features client,serial-rtu,discrete-inputs
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_device_id --no-default-features --features client,serial-rtu,diagnostics
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_backoff_jitter --no-default-features --features client,serial-rtu,coils
cargo run -p modbus-rs --example modbus_rs_client_async_serial_rtu --no-default-features --features async,serial-rtu,coils,registers

# Serial ASCII
cargo run -p modbus-rs --example modbus_rs_client_serial_ascii_coils --no-default-features --features client,serial-ascii,coils

# Gateway — sync TCP → RTU
cargo run -p modbus-rs --example modbus_rs_gateway_sync_tcp_to_rtu --no-default-features --features gateway,network-tcp,serial-rtu

# Gateway — async TCP → TCP
cargo run -p modbus-rs --example modbus_rs_gateway_async_tcp_to_tcp --no-default-features --features gateway,async,network-tcp
```

## Workspace Structure

- `modbus-rs`: top-level convenience crate
- `mbus-core`: shared protocol and transport abstractions
- `mbus-client`: client state machine and service modules
- `mbus-server`: server runtime, FC handlers, resilience engine
- `mbus-macros`: proc-macros (`#[modbus_app]`, `#[async_modbus_app]`, `#[derive(CoilsModel)]`, etc.)
- `mbus-async`: native async client and server via Tokio
- `mbus-gateway`: Modbus gateway — TCP ↔ RTU/ASCII/TCP routing (sync + async)
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