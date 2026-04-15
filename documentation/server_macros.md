# Server Macros Design

## Goals

The derive macros provide compile-time mapping between user-declared Rust structs
and Modbus protocol memory, while preserving stack ownership of all runtime buffers.

Goals:
1. User declares maps as plain Rust structs.
2. Mapping metadata is generated at compile time.
3. Runtime protocol buffers remain stack-owned.
4. Common mapping mistakes fail at compile time.

## Active Macro Surface

All four data-model derives are fully implemented:

| Derive                  | Function codes               | Direction    |
|-------------------------|------------------------------|--------------|
| `CoilsModel`            | FC01, FC05, FC0F             | read/write   |
| `HoldingRegistersModel` | FC03, FC06, FC10, FC16, FC17 | read/write   |
| `InputRegistersModel`   | FC04                         | read-only    |
| `DiscreteInputsModel`   | FC02                         | read-only    |
| `modbus_app`            | all of the above             | routing + validation |

### CoilsModel

1. Declares mapped `bool` coil fields via `#[coil(addr = N)]`
2. Generates `CoilMap` implementation used by `modbus_app`
3. Supports per-field write hooks and batch write hooks via `modbus_app`

### HoldingRegistersModel

1. Declares wire-ready `u16` register fields via `#[reg(addr = N)]`
2. Generates per-field getters/setters
3. Generates optional convenience helpers:
   - `field_scaled()` and `set_field_scaled()` when `scale` is present
   - `field_unit()` when `unit` is present
4. Generates `HoldingRegisterMap` implementation used by `modbus_app`
5. Supports per-field write hooks and batch write hooks via `modbus_app`

### InputRegistersModel

1. Declares wire-ready `u16` register fields via `#[reg(addr = N)]`
2. Generates per-field getters/setters for local model updates
3. Generates `InputRegisterMap` implementation used by `modbus_app`
4. No write trait methods — input registers are read-only from Modbus perspective
5. Same `scale`/`unit` optional helpers as `HoldingRegistersModel`

### DiscreteInputsModel

1. Declares mapped `bool` discrete input fields via `#[discrete_input(addr = N)]`
2. Generates `DiscreteInputMap` implementation with FC02 `encode()` support
3. Read-only bit-packed encoding (LSB-first per Modbus spec)
4. No write trait methods

## Attribute Grammar

### CoilsModel

Required:
1. `#[coil(addr = <u16>)]`

Optional:
1. `notify_via_batch = true` — directs single-write FC05 to the batch hook

Field type constraint:
1. Field type must be `bool`

### HoldingRegistersModel / InputRegistersModel

Required:
1. `#[reg(addr = <u16>)]`

Optional:
1. `scale = <number>` (must be > 0)
2. `unit = "..."`
3. `notify_via_batch = true` (HoldingRegistersModel only)

Field type constraint:
1. Field type must be `u16`

### DiscreteInputsModel

Required:
1. `#[discrete_input(addr = <u16>)]`

Field type constraint:
1. Field type must be `bool`

## Validation Rules

Compile-time validation enforces:
1. Coils / discrete inputs: duplicate addresses are rejected
2. Holding / input registers: duplicate register addresses are rejected
3. Missing required address attributes are rejected
4. Unsupported or malformed keys are rejected
5. Non-positive scale values are rejected
6. `modbus_app` map ranges: overlapping address ranges across multiple maps are rejected
7. `on_write_N` targets an address not covered by any selected map — compile error
8. `notify_via_batch` used without a configured `on_batch_write` hook — compile error

## Why this shape

A single derive path per data kind (`HoldingRegistersModel`, `InputRegistersModel`,
`CoilsModel`, `DiscreteInputsModel`) aligned with `modbus_app` routing avoids
parallel derive stacks with overlapping responsibilities and keeps compile-time
diagnostics targeted.

## Usage Example

```rust
use mbus_server::{CoilsModel, DiscreteInputsModel, HoldingRegistersModel, InputRegistersModel, modbus_app};

#[derive(Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    run: bool,
    #[coil(addr = 1)]
    fault_reset: bool,
}

#[derive(Default, DiscreteInputsModel)]
struct Inputs {
    #[discrete_input(addr = 0)]
    power_ok: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Holding {
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    setpoint: u16,
}

#[derive(Default, InputRegistersModel)]
struct Sensors {
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    temperature: u16,
}

#[derive(Default)]
#[modbus_app(
    coils(coils),
    discrete_inputs(inputs),
    holding_registers(holding),
    input_registers(sensors),
)]
struct App {
    coils: Coils,
    inputs: Inputs,
    holding: Holding,
    sensors: Sensors,
}
```

## Convenience helpers

Engineering-value and unit helpers are generated for `HoldingRegistersModel` and
`InputRegistersModel` when attributes are present:

- `field_scaled() -> f32` / `set_field_scaled(f32) -> Result<(), MbusError>` — requires `scale`
- `field_unit() -> &'static str` — requires `unit`

FC03/FC04/FC05/FC06/FC0F/FC10/FC16/FC17 routing is wired automatically via `modbus_app`.
