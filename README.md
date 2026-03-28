# modbus-rs

A complete, `no_std`-compatible Modbus client implementation in Rust, designed for embedded and resource-constrained environments. Compliant with the Modbus Application Protocol Specification V1.1b3 and Modbus Messaging on TCP/IP Implementation Guide V1.0b.

## Overview

`modbus-rs` is a Rust workspace providing a modular, poll-driven Modbus client stack. It is designed from the ground up to work without heap allocation and without blocking I/O, making it suitable for bare-metal embedded systems as well as standard OS applications.

Key properties:

- `no_std` compatible — uses `heapless` for fixed-capacity collections
- Poll-driven — no threads, no async runtime required
- Modular — pick only the transport and function-code features you need
- Configurable retry policy — exponential/linear/fixed backoff with optional app-provided jitter
- Supports Modbus TCP, Serial RTU, and Serial ASCII

## Workspace Structure

The workspace is split into focused crates:

| Crate | Role |
|---|---|
| [`mbus-core`](mbus-core/) | Shared protocol types, transport trait, config, errors, data models |
| [`mbus-client`](mbus-client/) | Client state machine, request/response orchestration, all function-code services |
| [`mbus-network`](mbus-network/) | Concrete TCP transport (`StdTcpTransport`) using `std::net::TcpStream` |
| [`mbus-serial`](mbus-serial/) | Concrete serial transport (`StdSerialTransport`) using the `serialport` crate |
| [`mbus-ffi`](mbus-ffi/) | WASM/JS bindings for browser-based Modbus over WebSocket and Web Serial |
| [`modbus-rs`](modbus-rs/) | Top-level convenience crate — re-exports everything behind feature flags |
| [`integration_tests`](integration_tests/) | Integration test suite across all transports |

For most applications, depend only on `modbus-rs`. Use the individual crates when you need lower-level control or a lighter dependency footprint.

## Quick Start

Add the full default setup (TCP + Serial RTU + Serial ASCII + all function codes):

```toml
[dependencies]
modbus-rs = "0.4.0"
```

Minimal TCP client with coil support only:

```toml
[dependencies]
modbus-rs = { version = "0.4.0", default-features = false, features = [
    "client",
    "tcp",
    "coils"
] }
```

See [documentation/quick_start.md](documentation/quick_start.md) for a complete walkthrough.

## Feature Flags

Feature flags are defined on the top-level `modbus-rs` crate and propagate into the workspace:

| Flag | Enables |
|---|---|
| `client` | `mbus-client` — request/response services |
| `tcp` | `mbus-network` — standard Modbus TCP transport |
| `serial-rtu` | `mbus-serial` for RTU framing |
| `serial-ascii` | `mbus-serial` for ASCII framing |
| `coils` | Read/write coil services |
| `registers` | Read/write holding and input register services |
| `discrete-inputs` | Read discrete input services |
| `fifo` | FIFO queue services |
| `file-record` | File record read/write services |
| `diagnostics` | Diagnostic and device identification services |
| `logging` | Enables `log` facade instrumentation in `mbus-network` and `mbus-serial` |

Default: all flags are enabled.

Note: WASM/browser APIs are provided by `mbus-ffi` directly. `modbus-rs` does not expose a
top-level `wasm` feature and does not re-export WASM types.

See [documentation/feature_flags.md](documentation/feature_flags.md) for valid combinations and build examples.

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

Use the same `ModbusSerialConfig` with `mode: SerialMode::Ascii` and pair it with `StdSerialTransport::new(SerialMode::Ascii)`.

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

See [documentation/feature_flags.md](documentation/feature_flags.md) or the [tcp_backoff_jitter_example](modbus-rs/examples/tcp_backoff_jitter_example.rs) and [serial_rtu_backoff_jitter_example](modbus-rs/examples/serial_rtu_backoff_jitter_example.rs) for full runnable examples.

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
modbus-rs = { version = "0.4.0", default-features = false, features = [
    "tcp",
    "logging"
] }
```

Run the logging example:

```bash
RUST_LOG=debug cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,logging
```

Filter only client internals at low priority:

```bash
RUST_LOG=mbus_client=trace cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,client,logging
```

## Examples

All examples live in [`modbus-rs/examples/`](modbus-rs/examples/) and are run from the workspace root.

### TCP

```bash
cargo run -p modbus-rs --example coils_example -- 192.168.1.10 502 1
cargo run -p modbus-rs --example registers_example -- 192.168.1.10 502 1
cargo run -p modbus-rs --example discrete_inputs_example -- 192.168.1.10 502 1
cargo run -p modbus-rs --example device_id_example -- 192.168.1.10 502 1
cargo run -p modbus-rs --example feature_facades_showcase --no-default-features --features client,tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
cargo run -p modbus-rs --example tcp_backoff_jitter_example -- 192.168.1.10 502 1
cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,logging
```

### Serial RTU

```bash
cargo run -p modbus-rs --example coils_serial_example --no-default-features --features client,serial-rtu,coils -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example registers_serial_example --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example discrete_inputs_serial_example --no-default-features --features client,serial-rtu,discrete-inputs -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example device_id_serial_example --no-default-features --features client,serial-rtu,diagnostics -- /dev/ttyUSB0 1
cargo run -p modbus-rs --example serial_rtu_backoff_jitter_example --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
```

### Serial ASCII

```bash
cargo run -p modbus-rs --example ascii_serial_example --no-default-features --features client,serial-ascii,coils -- /dev/ttyUSB0 1
```

## Architecture

The client follows the Modbus TCP Client Activity Diagram from the Modbus TCP/IP V1.0b specification and is extended with a poll-driven retry scheduler:

```
┌──────────────┐      ┌─────────────────────┐       ┌──────────────────┐
│  Your App    │─────▶│  ClientServices     │──────▶│  Transport trait │
│              │      │  (mbus-client)      │       │  (mbus-network /     │
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

Design principles:

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
cargo check -p modbus-rs --example tcp_backoff_jitter_example
```

## Documentation

- [documentation/quick_start.md](documentation/quick_start.md) — step-by-step setup guide
- [documentation/architecture.md](documentation/architecture.md) — state machine and design overview
- [documentation/feature_flags.md](documentation/feature_flags.md) — all feature flag combinations

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