# modbus-client

`modbus-client` is a helper crate in the `modbus-rs` workspace.

It provides the client-side Modbus request/response engine, built on top of shared
protocol and transport abstractions from `mbus-core`.

If you want a single top-level entry point, use `modbus-rs`.
If you want direct access to client orchestration and callbacks, use `modbus-client`.

## Helper Crate Role

`modbus-client` is responsible for client workflow, not transport implementation:

- Builds Modbus requests and tracks outstanding transactions.
- Polls transport for responses and dispatches parsed callbacks.
- Handles retries and timeout-based failure paths.
- Exposes feature-gated service modules by function group.

Transport implementations are provided by helper crates such as:
- `mbus-tcp`
- `mbus-serial`

## What Is Included

- `services::ClientServices`: the central client orchestrator.
- Feature-gated service modules:
  - `services::coil`
  - `services::register`
  - `services::discrete_input`
  - `services::fifo_queue`
  - `services::file_record`
  - `services::diagnostic`
- `app` callback traits:
  - `RequestErrorNotifier`
  - response traits for each function group

## Feature Flags

This crate uses selective compilation so you only build required protocol services.

Available features:

- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`

Default behavior:

- `default` enables all service features above.

Feature forwarding:

- Each feature forwards to the equivalent model feature in `mbus-core`.

Example (minimal feature set):

```toml
[dependencies]
modbus-client = { version = "0.1.0", default-features = false, features = ["coils"] }
```

## Usage Pattern

Typical flow:

1. Implement required callback traits in your app type.
2. Provide a `Transport` implementation (custom, `mbus-tcp`, or `mbus-serial`).
3. Build a `ModbusConfig`.
4. Construct `ClientServices`.
5. Issue requests.
6. Call `poll()` periodically to process responses and timeouts.

## Minimal Example

```rust
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};

use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use modbus_client::app::RequestErrorNotifier;
#[cfg(feature = "coils")]
use modbus_client::app::CoilResponse;
#[cfg(feature = "coils")]
use modbus_client::services::coil::Coils;
use modbus_client::services::ClientServices;

use heapless::Vec;

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

## Build Examples

From workspace root:

```bash
# default services
cargo check -p modbus-client

# only coils service
cargo check -p modbus-client --no-default-features --features coils

# registers + discrete inputs only
cargo check -p modbus-client --no-default-features --features registers,discrete-inputs
```

## Notes

- This crate is `no_std` friendly and uses `heapless` internally.
- Service and callback traits are conditionally compiled by feature flags.
- Use exact feature names with hyphens:
  - `discrete-inputs`
  - `file-record`

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
