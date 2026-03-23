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

These features gate model modules and related types.

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
