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
- `client`: Enables `mbus-client`.
- `serial-rtu`: Enables `mbus-serial` for RTU usage.
- `serial-ascii`: Enables `mbus-serial` for ASCII usage.
- `tcp`: Enables `mbus-network`.
- `async`: Enables `mbus-async` — the Tokio-based async facade (see below).
- `coils`: Enables coil model/service support.
- `registers`: Enables register model/service support.
- `discrete-inputs`: Enables discrete input model/service support.
- `fifo`: Enables FIFO queue model/service support.
- `file-record`: Enables file record model/service support.
- `diagnostics`: Enables diagnostics and device identification support.
- `traffic`: Enables raw TX/RX frame traffic hooks in `mbus-client` and async callback forwarding in `mbus-async`.
- `logging`: Enables `log` facade diagnostics in `mbus-network` and `mbus-serial`.

Default behavior:
- `default` currently enables: `client`, `serial-rtu`, `tcp`, and all function-group features above.
- The `async` feature is **not enabled by default** — add it explicitly when you need `.await` APIs.
- The `traffic` feature is **not enabled by default** — add it when you need raw frame observability.

Important:
- `serial-ascii` is available but **not** part of default features; enable it explicitly for ASCII builds.
- `modbus-rs` does not expose a top-level `wasm` feature. Browser and native C bindings are provided by `mbus-ffi`.

## Async Crate (`mbus-async`)

The `mbus-async` crate exposes [`AsyncTcpClient`] and [`AsyncSerialClient`] behind a Tokio
`.await` API. It is enabled through the top-level `async` feature.

Defined features:

| Feature | Enables |
|---|---|
| `tcp` | `AsyncTcpClient` — async TCP Modbus client |
| `serial-rtu` | RTU constructors on `AsyncSerialClient` |
| `serial-ascii` | ASCII constructors on `AsyncSerialClient` |
| `coils` | Coil service async methods |
| `registers` | Register service async methods |
| `discrete-inputs` | Discrete input async methods |
| `fifo` | FIFO queue async method |
| `file-record` | File record read/write async methods |
| `diagnostics` | Device identification, diagnostics, event log, report-server-id async methods |
| `traffic` | Async dedicated-thread callback API (`set_traffic_handler`) and traffic event forwarding |

Default features in `mbus-async`: `tcp`, `coils`, `registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`.

When you enable the `async` feature on `modbus-rs`, all of those function-group features are
forwarded from `modbus-rs` into `mbus-async` automatically — you do not need separate flag wiring.

### Async feature combinations

**Minimal async TCP client (coils only):**
```toml
modbus-rs = { version = "0.4", default-features = false, features = [
  "async", "tcp", "coils"
] }
tokio = { version = "1", features = ["full"] }
```

**Async TCP + all services (explicit):**
```toml
modbus-rs = { version = "0.4", default-features = false, features = [
  "async", "tcp", "coils", "registers", "discrete-inputs", "fifo", "file-record", "diagnostics"
] }
tokio = { version = "1", features = ["full"] }
```

**Async serial RTU (registers only):**
```toml
modbus-rs = { version = "0.4", default-features = false, features = [
  "async", "serial-rtu", "registers"
] }
tokio = { version = "1", features = ["full"] }
```

**Async serial ASCII with diagnostics:**
```toml
modbus-rs = { version = "0.4", default-features = false, features = [
  "async", "serial-ascii", "diagnostics"
] }
tokio = { version = "1", features = ["full"] }
```

## Client Crate (`mbus-client`)

Defined features:
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `traffic` (enables `TrafficNotifier` and `TrafficDirection`)

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

## Server Crate (`mbus-server`)

`mbus-server` has its own feature flags because it can be consumed directly without the
top-level `modbus-rs` crate.

Defined features:

- `coils`
- `holding-registers`
- `input-registers`
- `registers` (alias for `holding-registers` + `input-registers`)
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `diagnostics-stats`
- `traffic`
- `logging`
- `serial-ascii`

Notes:

- `diagnostics-stats` depends on `diagnostics`.
- `diagnostics-stats` is server-only; it is not exposed by the top-level `modbus-rs` crate.
- when enabled, FC08 counter/reporting sub-functions are handled by the server stack and backed by `ServerStatistics`

### `diagnostics-stats` behavior

With `diagnostics-stats` enabled, the server tracks protocol-level runtime counters for:

- valid message ingest
- communication/parse errors
- exception responses
- successful transmitted responses
- intentional no-response paths
- character overrun state

These counters are reported through FC08 diagnostics sub-functions such as:

- `0x000B` Return Bus Message Count
- `0x000C` Return Bus Communication Error Count
- `0x000D` Return Bus Exception Error Count
- `0x000E` Return Server Message Count
- `0x000F` Return Server No Response Count
- `0x0010` Return Server NAK Count
- `0x0011` Return Server Busy Count
- `0x0012` Return Bus Character Overrun Count
- `0x000A` Clear Counters and Diagnostic Register
- `0x0014` Clear Overrun Counter and Flag

Example:

```toml
[dependencies]
mbus-server = { version = "0.2.0", features = ["diagnostics-stats"] }
```

## Common Usage Patterns

### 1) Full default stack

```toml
[dependencies]
modbus-rs = "0.5.0"
```

### 2) Minimal client with only coils over TCP

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "tcp",
  "coils"
] }
```

### 3) Serial client with registers + discrete inputs

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "serial-rtu",
  "registers",
  "discrete-inputs"
] }
```

### 4) ASCII serial client with diagnostics

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "serial-ascii",
  "diagnostics"
] }
```

### 5) Diagnostics-only TCP build

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
  "client",
  "tcp",
  "diagnostics"
] }
```

### 6) TCP build with logging enabled

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
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

# Async TCP with all service features
cargo check --no-default-features --features async,tcp,coils,registers,discrete-inputs,fifo,file-record,diagnostics

# Async serial RTU
cargo check --no-default-features --features async,serial-rtu,coils,registers

# Sync traffic hooks
cargo check --no-default-features --features client,tcp,coils,traffic

# Async traffic hooks
cargo check --no-default-features --features async,tcp,coils,traffic

# Run async TCP example
cargo run --package modbus-rs --example modbus_rs_client_async_tcp --features async

# Run async serial RTU example
cargo run --package modbus-rs --example modbus_rs_client_async_serial_rtu --no-default-features --features async,serial-rtu,coils,registers

# Run sync traffic example
cargo run --package modbus-rs --example modbus_rs_client_traffic_sync_tcp --features traffic

# Run async traffic example
cargo run --package modbus-rs --example modbus_rs_client_traffic_async_tcp --features async,traffic
```

## Logging Setup

`logging` only enables instrumentation points via the `log` facade. Your application
must initialize a logger backend to see output.

Logging coverage:

- `mbus-network`: transport connection and socket diagnostics
- `mbus-serial`: serial transport diagnostics
- `mbus-client`: low-priority internal state-machine events (`debug`/`trace`), such as frame resync, retry scheduling, timeout handling, and connection-loss flushing

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
RUST_LOG=debug cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,logging
```

Filter only internal client state-machine logs:

```bash
RUST_LOG=mbus_client=trace cargo run -p modbus-rs --example modbus_rs_client_tcp_logging --no-default-features --features tcp,client,logging
```

## Bindings Note (WASM and C)

Feature flags documented here apply to the Rust API crates.

For bindings:

- WASM/browser bindings are provided by `mbus-ffi` (`mbus-ffi/src/wasm/`, package output in `mbus-ffi/pkg/`).
- Native C bindings are provided by `mbus-ffi` (`mbus-ffi/include/mbus_ffi.h`, C API in `mbus-ffi/src/c/`).

Useful binding validation commands:

```bash
# Rust-side FFI tests
cargo test -p mbus-ffi

# Native C smoke flow
cargo run -p xtask -- build-c-smoke
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

`mbus-client` now provides additional operational APIs:

- `ClientServices::connect()`
- `ClientServices::is_connected()`
- `ClientServices::reconnect()`
- `ClientServices::new_serial(...)`
- `SerialClientServices<TRANSPORT, APP>` alias

These are runtime behavior APIs (not Cargo features), but they are relevant when
designing feature-reduced builds and deployment behavior.
