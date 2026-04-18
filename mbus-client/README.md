# mbus-client

Modbus client state machine for Rust — poll-driven, no_std compatible, zero heap allocation.

[![crates.io](https://img.shields.io/crates/v/mbus-client)](https://crates.io/crates/mbus-client)
[![docs.rs](https://docs.rs/mbus-client/badge.svg)](https://docs.rs/mbus-client)

## Features

- **Poll-driven** — no threads, no blocking I/O
- **no_std compatible** — runs on embedded MCUs
- **Transport agnostic** — TCP, Serial RTU, Serial ASCII
- **All standard FCs** — coils, registers, discrete inputs, FIFO, file records, diagnostics
- **Configurable retry** — exponential/linear/fixed backoff with optional jitter

## Quick Start

```rust
use mbus_client::{ClientServices, ModbusTcpConfig, StdTcpTransport};

let config = ModbusTcpConfig::new("192.168.1.10", 502)?;
let mut client = ClientServices::<_, _, 4>::new(
    StdTcpTransport::new(),
    app,
    config.into()
)?;

client.connect()?;
client.coils().read_coils(1, unit_id, 0, 16)?;

loop {
    client.poll();  // Drive state machine
}
```

## Documentation

📖 **[Full Documentation](https://github.com/Raghava-Ch/modbus-rs/tree/main/documentation/client)**

| Topic | Link |
|-------|------|
| Quick Start | [documentation/client/quick_start.md](https://github.com/Raghava-Ch/modbus-rs/blob/main/documentation/client/quick_start.md) |
| Building Apps | [documentation/client/building_applications.md](https://github.com/Raghava-Ch/modbus-rs/blob/main/documentation/client/building_applications.md) |
| Feature Flags | [documentation/client/feature_flags.md](https://github.com/Raghava-Ch/modbus-rs/blob/main/documentation/client/feature_flags.md) |
| Architecture | [documentation/client/architecture.md](https://github.com/Raghava-Ch/modbus-rs/blob/main/documentation/client/architecture.md) |

## Related Crates

| Crate | Purpose |
|-------|---------|
| [`modbus-rs`](https://crates.io/crates/modbus-rs) | Top-level convenience crate |
| [`mbus-core`](https://crates.io/crates/mbus-core) | Shared protocol types |
| [`mbus-async`](https://crates.io/crates/mbus-async) | Tokio async facade |
| [`mbus-network`](https://crates.io/crates/mbus-network) | TCP transport |
| [`mbus-serial`](https://crates.io/crates/mbus-serial) | Serial RTU/ASCII transport |


- Runtime-safe path: `ClientServices::new(...)` validates serial `N == 1`.
- Compile-time-safe path: `ClientServices::new_serial(...)` enforces `N == 1`.
- Recommended type alias: `SerialClientServices<TRANSPORT, APP>`.

## Feature Flags

This crate uses selective compilation so you only build required protocol services.

Available features:

- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `serial-ascii` (forwards to `mbus-core/serial-ascii` to enable ASCII-sized ADU buffers)
- `traffic` (enables raw TX/RX frame callbacks via `TrafficNotifier`)
- `logging` (enables low-priority internal state-machine diagnostics via the `log` facade)

Default behavior:

- `default` enables all service features above.

Feature forwarding:

- Each feature forwards to the equivalent model feature in `mbus-core`.

Example (minimal feature set):

```toml
[dependencies]
mbus-client = { version = "0.6.0", default-features = false, features = ["coils"] }
```

## Traffic Callbacks (optional `traffic` feature)

When `traffic` is enabled, apps can implement `TrafficNotifier` to observe raw ADU frames:

<!-- validate: compile -->
```rust
use mbus_client::app::{TrafficDirection, TrafficNotifier};
use mbus_core::transport::UnitIdOrSlaveAddr;

struct App;

impl TrafficNotifier for App {
  fn on_tx_frame(
    &mut self,
    txn_id: u16,
    unit_id_slave_addr: UnitIdOrSlaveAddr,
    frame_bytes: &[u8],
  ) {
    println!(
      "[{:?}] txn={} unit={} bytes={:02X?}",
      TrafficDirection::Tx,
      txn_id,
      unit_id_slave_addr.get(),
      frame_bytes
    );
  }

  fn on_rx_frame(
    &mut self,
    txn_id: u16,
    unit_id_slave_addr: UnitIdOrSlaveAddr,
    frame_bytes: &[u8],
  ) {
    println!(
      "[{:?}] txn={} unit={} bytes={:02X?}",
      TrafficDirection::Rx,
      txn_id,
      unit_id_slave_addr.get(),
      frame_bytes
    );
  }

  fn on_tx_error(
    &mut self,
    txn_id: u16,
    unit_id_slave_addr: UnitIdOrSlaveAddr,
    error: mbus_core::errors::MbusError,
    frame_bytes: &[u8],
  ) {
    println!(
      "[{:?}] txn={} unit={} error={error:?} bytes={:02X?}",
      TrafficDirection::Tx,
      txn_id,
      unit_id_slave_addr.get(),
      frame_bytes
    );
  }

  fn on_rx_error(
    &mut self,
    txn_id: u16,
    unit_id_slave_addr: UnitIdOrSlaveAddr,
    error: mbus_core::errors::MbusError,
    frame_bytes: &[u8],
  ) {
    println!(
      "[{:?}] txn={} unit={} error={error:?} bytes={:02X?}",
      TrafficDirection::Rx,
      txn_id,
      unit_id_slave_addr.get(),
      frame_bytes
    );
  }
}
```

## Logging

`mbus-client` can emit low-priority internal diagnostics through the `log` facade when the
`logging` feature is enabled.

These logs are intentionally limited to `debug` and `trace` so applications can filter them
without treating normal control-flow events as warnings or errors.

Examples of logged events:

- frame parse/resynchronization
- response dispatch matching
- timeout scans and retry scheduling
- retry send failures
- pending-request flush during connection loss or reconnect

Typical filtering example:

```bash
RUST_LOG=mbus_client=trace cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,client,logging
```

## Usage Pattern

Typical flow:

1. Implement required callback traits in your app type.
2. Provide a `Transport` implementation (custom, `mbus-network`, or `mbus-serial`).
3. Build a `ModbusConfig`.
4. Construct `ClientServices`.
5. Issue requests.
6. Call `poll()` periodically to process responses and timeouts.

## Minimal Example

```rust
use modbus_rs::{
  ClientServices, MAX_ADU_FRAME_LEN, MbusError, ModbusConfig, ModbusTcpConfig,
  RequestErrorNotifier, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};
#[cfg(feature = "coils")]
use modbus_rs::{CoilResponse, Coils};

use heapless::Vec;

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

    #[cfg(feature = "coils")]
    client.with_coils(|coils| {
      coils.read_single_coil(2, UnitIdOrSlaveAddr::new(1)?, 0)?;
      coils.write_single_coil(3, UnitIdOrSlaveAddr::new(1)?, 0, true)?;
      Ok::<(), MbusError>(())
    })?;

    while client.has_pending_requests() {
        client.poll();
    }
    Ok(())
}
```

  ## Feature-Scoped Access Style

  `ClientServices` now supports feature facades so request APIs can be grouped by domain:

  - `client.coils()`
  - `client.registers()`
  - `client.discrete_inputs()`
  - `client.diagnostic()`
  - `client.fifo()`
  - `client.file_records()`

  For grouped request submission in a single scoped borrow, use batch helpers:

  - `client.with_coils(...)`
  - `client.with_registers(...)`
  - `client.with_discrete_inputs(...)`
  - `client.with_diagnostic(...)`
  - `client.with_fifo(...)`
  - `client.with_file_records(...)`

## Build Examples

From workspace root:

```bash
# default services
cargo check -p mbus-client

# only coils service
cargo check -p mbus-client --no-default-features --features coils

# registers + discrete inputs only
cargo check -p mbus-client --no-default-features --features registers,discrete-inputs
```

## Notes

- This crate is `no_std` friendly and uses `heapless` internally.
- Service and callback traits are conditionally compiled by feature flags.
- Use exact feature names with hyphens:
  - `discrete-inputs`
  - `file-record`

## License

This crate is licensed under **GPL-3.0-only**.

If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

## Disclaimer

This is an independent Rust implementation of the Modbus specification and is not
affiliated with the Modbus Organization.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)
