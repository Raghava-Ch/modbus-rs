# Feature Flags Guide

This document explains how to enable and disable features across the `modbus-rs` workspace.

## Why Use Feature Flags

Feature flags help you:
- Reduce binary size by compiling only required modules.
- Avoid pulling transport dependencies you do not use.
- Build a minimal Modbus client for embedded targets.

## Top-Level Crate (`modbus-rs`)

The `modbus-rs` crate is the main entry point and re-exports sub-crates.

Defined features:
- `client`: Enables `modbus-client`.
- `serial-rtu`: Enables `mbus-serial` for RTU usage.
- `serial-ascii`: Enables `mbus-serial` for ASCII usage.
- `tcp`: Enables `mbus-tcp`.
- `coils`: Enables coil model/service support.
- `registers`: Enables register model/service support.
- `discrete-inputs`: Enables discrete input model/service support.
- `fifo`: Enables FIFO queue model/service support.
- `file-record`: Enables file record model/service support.
- `diagnostics`: Enables diagnostics and device identification support.
- `logging`: Enables `log` facade diagnostics in `mbus-tcp` and `mbus-serial`.

Default behavior:
- `default` currently enables: `client`, `serial-rtu`, `serial-ascii`, `tcp`, and all function-group features above.

## Client Crate (`modbus-client`)

Defined features:
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`

Each feature forwards to its equivalent in `mbus-core`.

## Core Crate (`mbus-core`)

Defined features:
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `serial-ascii`

These features gate model modules and related types.

`serial-ascii` also controls ADU buffer sizing in `mbus-core`:

- enabled: `MAX_ADU_FRAME_LEN = 513`
- disabled: `MAX_ADU_FRAME_LEN = 260`

This optimization reduces stack usage for builds that do not include ASCII transport.

## Common Usage Patterns

### 1) Full default stack

```toml
[dependencies]
modbus-rs = "0.1.0"
```

### 2) Minimal client with only coils over TCP

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

### 3) Serial client with registers + discrete inputs

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "serial-rtu",
  "registers",
  "discrete-inputs"
] }
```

### 4) ASCII serial client with diagnostics

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "serial-ascii",
  "diagnostics"
] }
```

### 5) Diagnostics-only TCP build

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "client",
  "tcp",
  "diagnostics"
] }
```

### 6) TCP build with logging enabled

```toml
[dependencies]
modbus-rs = { version = "0.1.0", default-features = false, features = [
  "tcp",
  "logging"
] }
```

## CLI Build Examples

From the workspace root:

```bash
# Build everything (default features)
cargo check

# Build only client + tcp + coils
cargo check --no-default-features --features client,tcp,coils

# Build only client + RTU serial + registers + discrete inputs
cargo check --no-default-features --features client,serial-rtu,registers,discrete-inputs

# Build only client + ASCII serial + diagnostics
cargo check --no-default-features --features client,serial-ascii,diagnostics

# Build only TCP transport + logging
cargo check --no-default-features --features tcp,logging
```

## Logging Setup

`logging` only enables instrumentation points via the `log` facade. Your application
must initialize a logger backend to see output.

Logging coverage:

- `mbus-tcp`: transport connection and socket diagnostics
- `mbus-serial`: serial transport diagnostics
- `modbus-client`: low-priority internal state-machine events (`debug`/`trace`), such as frame resync, retry scheduling, timeout handling, and connection-loss flushing

Typical std setup:

```toml
[dependencies]
env_logger = "0.11"
```

```rust
env_logger::init();
```

Then run with a log level (example):

```bash
RUST_LOG=debug cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,logging
```

Filter only internal client state-machine logs:

```bash
RUST_LOG=modbus_client=trace cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,client,logging
```

## Notes About Feature Names

Use hyphenated names exactly as defined in `Cargo.toml`:
- `discrete-inputs` (not `discrete_inputs`)
- `file-record` (not `file_records`)

## Future Server Feature

A dedicated `server` feature is planned but is not implemented yet in the current workspace.

Suggested future pattern:
- `server`: Enable server-side protocol state machine and services.
- Function-group flags can be shared where possible between client/server modules.

## Troubleshooting

If a type or trait is missing at compile time:
1. Ensure the matching feature is enabled in your dependency declaration.
2. If using `default-features = false`, verify you included `client` and one transport (`tcp`, `serial-rtu`, or `serial-ascii`) when needed.
3. Re-run with explicit features:

```bash
cargo check --no-default-features --features client,tcp,coils
```

## Retry Backoff and Jitter

Retry timing is configured per transport config (`ModbusTcpConfig` and `ModbusSerialConfig`):

- `retry_backoff_strategy`
- `retry_jitter_strategy`
- `retry_random_fn`

Defaults preserve previous behavior:

- `retry_backoff_strategy = BackoffStrategy::Immediate`
- `retry_jitter_strategy = JitterStrategy::None`
- `retry_random_fn = None`

Important operational model:

- Retries are scheduled and executed from `poll()`.
- No internal sleeping/blocking is used.
- Jitter uses only the app-provided callback and is skipped when no callback is provided.

## Reconnect and Serial Constructor APIs

`modbus-client` now provides additional operational APIs:

- `ClientServices::is_connected()`
- `ClientServices::reconnect()`
- `ClientServices::new_serial(...)`
- `SerialClientServices<TRANSPORT, APP>` alias

These are runtime behavior APIs (not Cargo features), but they are relevant when
designing feature-reduced builds and deployment behavior.
