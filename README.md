# modbus-rs

`modbus-rs` is a low-footprint, cross-platform Modbus client workspace built for both embedded and desktop/server systems.
It runs on no_std and std targets (MCUs, RTOS, Windows, Linux, macOS), supports TCP/RTU/ASCII, provides sync and async APIs, and uses feature gating to keep binaries minimal.
Advanced reliability features include configurable retry, backoff, and jitter, with optional native C and WASM bindings via `mbus-ffi`.

## Overview

`modbus-rs` is a Rust workspace providing a modular, poll-driven Modbus client stack. It is designed from the ground up to work without heap allocation and without blocking I/O, making it suitable for bare-metal embedded systems as well as standard OS applications.

Key properties:

- `no_std` compatible — uses `heapless` for fixed-capacity collections
- Poll-driven — no threads, no async runtime required
- Optional async facade — `mbus-async` provides Tokio-based `.await` APIs
- Modular — pick only the transport and function-code features you need
- Configurable retry policy — exponential/linear/fixed backoff with optional app-provided jitter
- Supports Modbus TCP, Serial RTU, and Serial ASCII

## Workspace Structure

The workspace is split into focused crates:

| Crate | Role |
|---|---|
| [`mbus-core`](mbus-core/) | Shared protocol types, transport trait, config, errors, data models |
| [`mbus-client`](mbus-client/) | Client state machine, request/response orchestration, all function-code services |
| [`mbus-async`](mbus-async/) | Tokio async facade over `mbus-client` (`AsyncTcpClient`, `AsyncSerialClient`) |
| [`mbus-network`](mbus-network/) | Concrete TCP transport (`StdTcpTransport`) using `std::net::TcpStream` |
| [`mbus-serial`](mbus-serial/) | Concrete serial transports (`StdRtuTransport`, `StdAsciiTransport`) using the `serialport` crate |
| [`mbus-ffi`](mbus-ffi/) | Native C bindings and WASM/browser bindings |
| [`mbus-server`](mbus-server/) | Server-side workspace crate (currently minimal scaffolding) |
| [`modbus-rs`](modbus-rs/) | Top-level convenience crate — re-exports everything behind feature flags |
| [`integration_tests`](integration_tests/) | Integration test suite across all transports |
| [`xtask`](xtask/) | Workspace automation tasks (header checks, C smoke build/test) |

For most applications, depend only on `modbus-rs`. Use the individual crates when you need lower-level control or a lighter dependency footprint.

## Quick Start

Add the full default setup (TCP + Serial RTU + Serial ASCII + all function codes):

```toml
[dependencies]
modbus-rs = "0.5.0"
```

Minimal TCP client with coil support only:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "tcp",
    "coils"
] }
```

See [documentation/quick_start.md](documentation/quick_start.md) for a complete walkthrough.

## Project Docs

- [documentation/quick_start.md](documentation/quick_start.md) — usage and setup walkthrough
- [documentation/feature_flags.md](documentation/feature_flags.md) — feature combinations and build examples
- [documentation/architecture.md](documentation/architecture.md) — architecture and runtime model
- [documentation/migration_guide.md](documentation/migration_guide.md) — breaking-change migration steps (Rust + C/FFI)
- [CONTRIBUTING.md](CONTRIBUTING.md) — contribution workflow and validation steps
- [RELEASE.md](RELEASE.md) — release checklist
- [mbus-ffi/README.md](mbus-ffi/README.md) — WASM and native C binding docs
- [modbus-rs/README.md](modbus-rs/README.md) — top-level crate API guide
- [mbus-server/README.md](mbus-server/README.md) — server derive/macros, write hooks, compile-time validation diagnostics, and server-only feature flags such as `diagnostics-stats`
- [mbus-server/examples/discrete_inputs_model.rs](mbus-server/examples/discrete_inputs_model.rs) — minimal `DiscreteInputsModel` usage example

## Feature Flags

Feature flags are defined on the top-level `modbus-rs` crate and propagate into the workspace:

| Flag | Enables |
|---|---|
| `client` | `mbus-client` — request/response services |
| `tcp` | `mbus-network` — standard Modbus TCP transport |
| `serial-rtu` | `mbus-serial` for RTU framing |
| `serial-ascii` | `mbus-serial` for ASCII framing |
| `async` | `mbus-async` — Tokio async client facade |
| `coils` | Read/write coil services |
| `registers` | Read/write holding and input register services |
| `discrete-inputs` | Read discrete input services |
| `fifo` | FIFO queue services |
| `file-record` | File record read/write services |
| `diagnostics` | Diagnostic and device identification services |
| `traffic` | Raw TX/RX frame traffic callbacks/hooks for sync and async clients |
| `logging` | Enables `log` facade instrumentation in `mbus-network` and `mbus-serial` |

Default: `client`, `serial-rtu`, `tcp`, and all function-group flags are enabled.
`serial-ascii`, `async`, `traffic`, and `logging` are opt-in.

`async` is optional and should be enabled explicitly when using `.await` APIs.

Note: WASM/browser APIs and native C bindings are provided by `mbus-ffi` directly.
`modbus-rs` does not expose a top-level `wasm` feature and does not re-export WASM or C binding APIs.

Note: server-only features such as `diagnostics-stats` live on the `mbus-server` crate,
not on the top-level `modbus-rs` crate.

See [documentation/feature_flags.md](documentation/feature_flags.md) for valid combinations and build examples.

## Async Usage

Async clients are exposed by `mbus-async` and re-exported via `modbus_rs::mbus_async` when the
`async` feature is enabled.

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "async",
    "tcp",
    "coils"
] }
tokio = { version = "1", features = ["full"] }
```

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

For serial async clients, use `AsyncSerialClient::new_rtu(...)` or `AsyncSerialClient::new_ascii(...)`,
then call `client.connect().await?` before sending requests.

### Traffic Observability

Enable `traffic` to observe raw TX/RX ADU frames in both sync and async flows.

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

- Sync: `modbus-rs/examples/modbus-rs/client/traffic/traffic_sync_example.rs`
- Async: `modbus-rs/examples/modbus-rs/client/traffic/traffic_async_tcp_example.rs`

## Bindings (WASM and C)

Bindings are provided by [`mbus-ffi`](mbus-ffi/) and are intended for browser and native integration use cases.

WASM/browser bindings:

- Docs and usage: [mbus-ffi/README.md](mbus-ffi/README.md)
- Browser smoke pages:
    - `mbus-ffi/examples/network_smoke.html`
    - `mbus-ffi/examples/serial_smoke.html`
- Package output path: `mbus-ffi/pkg/`
- WASM-facing source modules: `mbus-ffi/src/wasm/`

Native C bindings:

- C header: `mbus-ffi/include/mbus_ffi.h`
- C API source: `mbus-ffi/src/c/`
- Native C smoke project: `mbus-ffi/examples/c_smoke_cmake/`
- Standalone C binding-layer tests: `mbus-ffi/tests/c_api/test_binding_layer.c`

If your target is browser JavaScript or native C/C++, start from `mbus-ffi` docs first.

## Basic Usage

The following skeleton works with any transport. The `App` struct implements the response callbacks and provides the timestamp:

```rust
use modbus_rs::{
    ClientServices, MAX_ADU_FRAME_LEN, MbusError, ModbusConfig, ModbusTcpConfig,
    RequestErrorNotifier, TimeKeeper, Transport, TransportType, UnitIdOrSlaveAddr,
};
use modbus_rs::heapless::Vec;

#[cfg(feature = "registers")]
use modbus_rs::{RegisterResponse, Registers};

struct MyTransport { /* ... */ }

impl Transport for MyTransport {
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
    fn request_failed(&self, _: u16, _: UnitIdOrSlaveAddr, _: MbusError) {}
}

#[cfg(feature = "registers")]
impl RegisterResponse for App {
    fn read_holding_registers_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: &Registers) {}
    fn read_input_registers_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: &Registers) {}
    fn write_single_register_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn write_multiple_registers_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl TimeKeeper for App {
    fn current_millis(&self) -> u64 {
        // return wall-clock milliseconds from your platform timer
        0
    }
}

fn main() -> Result<(), MbusError> {
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
    let mut client = ClientServices::<_, _, 4>::new(MyTransport, App, config)?;
    client.connect()?;

    #[cfg(feature = "registers")]
    client
        .registers()
        .read_holding_registers(1, UnitIdOrSlaveAddr::new(1)?, 0, 10)?;

    loop {
        client.poll();
        // on embedded: wait for next timer tick; on std: yield/sleep briefly
    }
}
```

## Transport Configuration

### TCP

```rust
use modbus_rs::ModbusTcpConfig;

let config = ModbusTcpConfig::new("192.168.1.10", 502)?;
// config.response_timeout_ms = 1500;
// config.retry_attempts = 3;
```

### Serial RTU

```rust
use modbus_rs::{BaudRate, DataBits, ModbusSerialConfig, Parity, SerialMode};

let config = ModbusSerialConfig {
    port_path: "/dev/ttyUSB0".try_into()?,
    mode: SerialMode::Rtu,
    baud_rate: BaudRate::Baud19200,
    data_bits: DataBits::Eight,
    stop_bits: 1,
    parity: Parity::Even,
    response_timeout_ms: 1000,
    retry_attempts: 3,
    retry_backoff_strategy: Default::default(),
    retry_jitter_strategy: Default::default(),
    retry_random_fn: None,
};
```

### Serial ASCII

Use the same `ModbusSerialConfig` with `mode: SerialMode::Ascii` and pair it with `StdAsciiTransport::new()`.

## Retry Backoff and Jitter

All transport configs support a configurable retry policy with no blocking or internal RNG:

```rust
use modbus_rs::{BackoffStrategy, JitterStrategy, ModbusTcpConfig};

let mut config = ModbusTcpConfig::new("192.168.1.10", 502)?;
config.retry_attempts = 5;
config.retry_backoff_strategy = BackoffStrategy::Exponential {
    base_delay_ms: 100,
    max_delay_ms: 3000,
};
config.retry_jitter_strategy = JitterStrategy::Percentage { percent: 20 };
config.retry_random_fn = Some(my_platform_random_u32); // fn() -> u32
```

Available strategies:

| `BackoffStrategy` | Behaviour |
|---|---|
| `Immediate` | Retry without delay (default) |
| `Fixed { delay_ms }` | Same delay for every retry |
| `Exponential { base_delay_ms, max_delay_ms }` | Delay doubles each attempt, capped at max |
| `Linear { initial_delay_ms, increment_ms, max_delay_ms }` | Delay grows linearly per attempt |

| `JitterStrategy` | Behaviour |
|---|---|
| `None` | No jitter added (default) |
| `Percentage { percent }` | Adds up to `percent`% of the base delay |
| `BoundedMs { max_jitter_ms }` | Adds up to `max_jitter_ms` uniformly |

When `retry_random_fn` is `None`, jitter is skipped and the base delay is used as-is. This lets you defer RNG setup without changing the rest of the config.

See [documentation/feature_flags.md](documentation/feature_flags.md) or the [tcp_backoff_jitter_example](modbus-rs/examples/modbus-rs/client/tcp/tcp_backoff_jitter_example.rs) and [serial_rtu_backoff_jitter_example](modbus-rs/examples/modbus-rs/client/serial/serial_rtu_backoff_jitter_example.rs) for full runnable examples.

## Logging

The workspace now uses the `log` facade for transport diagnostics instead of writing directly to stderr.

- Logging is optional and enabled with the `logging` feature.
- The crate remains `no_std` compatible because `log` is used as a facade and does not require a logger backend at compile time.
- Your application selects and initializes a logger implementation (for example `env_logger` on std targets).
- Transport diagnostics use `debug`/`warn`/`error`.
- Internal `mbus-client` state-machine diagnostics use low-priority `debug`/`trace` events so they remain filterable.

Enable logging with TCP only:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "tcp",
    "logging"
] }
```

Run the logging example:

```bash
RUST_LOG=debug cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,logging
```

Filter only client internals at low priority:

```bash
RUST_LOG=mbus_client=trace cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,client,logging
```

## Examples

All examples live in [`modbus-rs/examples/`](modbus-rs/examples/) and are run from the workspace root.

### Async (Rust)

- [modbus-rs/examples/modbus-rs/client/async/async_tcp_example.rs](modbus-rs/examples/modbus-rs/client/async/async_tcp_example.rs)
- [modbus-rs/examples/modbus-rs/client/async/async_serial_rtu_example.rs](modbus-rs/examples/modbus-rs/client/async/async_serial_rtu_example.rs)

```bash
# Async TCP
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --features async

# Async serial RTU
cargo run -p modbus-rs --example modbus_rs_client_async_serial_rtu --no-default-features --features async,serial-rtu,coils,registers

# Sync traffic callback demo
cargo run -p modbus-rs --example modbus_rs_client_traffic_sync_tcp --features traffic

# Async traffic callback demo
cargo run -p modbus-rs --example modbus_rs_client_traffic_async_tcp --features async,traffic
```

### TCP

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 192.168.1.10 502 1
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers -- 192.168.1.10 502 1
cargo run -p modbus-rs --example modbus_rs_client_tcp_discrete_inputs -- 192.168.1.10 502 1
cargo run -p modbus-rs --example modbus_rs_client_tcp_device_id -- 192.168.1.10 502 1
cargo run -p modbus-rs --example modbus_rs_client_showcase_feature_facades --no-default-features --features client,tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter -- 192.168.1.10 502 1
cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,logging
```

### Serial RTU

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_coils --no-default-features --features client,serial-rtu,coils -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_discrete_inputs --no-default-features --features client,serial-rtu,discrete-inputs -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_device_id --no-default-features --features client,serial-rtu,diagnostics -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_backoff_jitter --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
```

### Serial ASCII

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_ascii_coils --no-default-features --features client,serial-ascii,coils -- /dev/ttyUSB0 1
```

### WASM Browser Smoke Examples (`mbus-ffi`)

- [mbus-ffi/examples/network_smoke.html](mbus-ffi/examples/network_smoke.html)
- [mbus-ffi/examples/serial_smoke.html](mbus-ffi/examples/serial_smoke.html)

```bash
cd mbus-ffi
wasm-pack build --target web --features wasm,full
python3 -m http.server 8089 --directory ./examples/
```

### Native C Binding Examples (`mbus-ffi`)

- [mbus-ffi/examples/c_smoke_cmake/main.c](mbus-ffi/examples/c_smoke_cmake/main.c)
- [mbus-ffi/tests/c_api/test_binding_layer.c](mbus-ffi/tests/c_api/test_binding_layer.c)

```bash
# Build and run native C smoke test path
cargo run -p xtask -- build-c-smoke
```



## Architecture

The client follows the Modbus TCP Client Activity Diagram from the Modbus TCP/IP V1.0b specification and is extended with a poll-driven retry scheduler:

```
┌──────────────┐      ┌─────────────────────┐       ┌──────────────────┐
│  Your App    │─────▶│  ClientServices     │──────▶│  Transport trait │
│              │      │  (mbus-client)      │       │  (mbus-network / │
│  poll() loop │◀─────│  request queue,     │       │   mbus-serial)   │
│  callbacks   │      │  retry scheduler,   │       └──────────────────┘
│  TimeKeeper  │      │  timeout tracking   │
└──────────────┘      └─────────────────────┘
                               │
                               ▼
                      ┌─────────────────────┐
                      │  mbus-core          │
                      │  protocol types,    │
                      │  ADU/PDU framing,   │
                      │  error model        │
                      └─────────────────────┘
```

Core Design principles:

- **No internal state machine threads** — all progress is driven by `client.poll()`
- **No heap allocation** — queue depth `N` is a compile-time const generic on `ClientServices`
- **No internal RNG** — jitter requires an app-provided `fn() -> u32` callback
- **Transport-agnostic** — swap TCP for serial or a mock by changing the generic parameter
- **Backoff is timestamp-scheduled** — `poll()` checks `current_millis()` and skips early retries without blocking

See [documentation/architecture.md](documentation/architecture.md) for the full state diagram and design notes.

## Building and Testing

```bash
# Check entire workspace
cargo check --workspace

# Run all tests
cargo test --workspace

# Run only client unit tests
cargo test -p mbus-client

# Run integration tests
cargo test -p integration_tests

# Check a specific example
cargo check -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter

# Compile all modbus-rs examples
cargo check -p modbus-rs --examples --all-features

# FFI native C smoke test
cargo run -p xtask -- build-c-smoke

# FFI Rust-side tests
cargo test -p mbus-ffi
```

## Documentation

- [documentation/quick_start.md](documentation/quick_start.md) — step-by-step setup guide
- [documentation/architecture.md](documentation/architecture.md) — state machine and design overview
- [documentation/feature_flags.md](documentation/feature_flags.md) — all feature flag combinations
- [mbus-ffi/README.md](mbus-ffi/README.md) — WASM and native C bindings

## Licensing

Copyright (C) 2025 Raghava Challari

This project is currently licensed under the GNU General Public License v3.0 (GPLv3) for evaluation purposes.

For details, refer to the [LICENSE](./LICENSE) file or the [GPLv3 official site](https://www.gnu.org/licenses/gpl-3.0.en.html).

## Disclaimer

This is an independent Rust implementation of the Modbus specification and is not affiliated with the Modbus Organization or Modbus Consortium.

## Contact

**Name:** Raghava Ch  
**Email:** [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)  
**Repository:** [github.com/Raghava-Ch/modbus-rs](https://github.com/Raghava-Ch/modbus-rs)