# Server Macro Phase 1 Design

## Goals

Phase 1 introduces derive-driven mapping for coils and holding registers while preserving stack ownership of mutable protocol memory.

Goals:
1. User declares maps as plain Rust structs.
2. Mapping metadata is generated at compile time.
3. Runtime protocol buffers remain stack-owned.
4. Common mapping mistakes fail at compile time.

## Scope

Included in phase 1:
1. `CoilsModel` derive
2. `HoldingRegistersModel` derive
3. `modbus_app` routing generation
4. Compile-time descriptor generation and validation
5. Generated convenience helper methods

Out of scope in phase 1:
1. Input-register-specific derive
2. Full server runtime completion for all function groups

## Active Macro Surface

### Coils

`CoilsModel`:
1. Declares mapped `bool` coil fields via `#[coil(addr = N)]`
2. Generates `CoilMap` implementation used by `modbus_app`

### Holding registers

`HoldingRegistersModel`:
1. Declares wire-ready `u16` register fields via `#[reg(addr = N)]`
2. Generates per-field getters/setters
3. Generates optional convenience helpers:
4. `field_scaled()` and `set_field_scaled()` when `scale` is present
5. `field_unit()` when `unit` is present
6. Generates `HoldingRegisterMap` implementation used by `modbus_app`

## Attribute Grammar

### Coils

Required:
1. `#[coil(addr = <u16>)]`

Field type constraint:
1. Field type must be `bool`

### HoldingRegisters

Required:
1. `#[reg(addr = <u16>)]`

Optional:
1. `scale = <number>` (must be > 0)
2. `unit = "..."`

Field type constraint:
1. Field type must be `u16`

## Validation Rules

Compile-time validation enforces:
1. Coils: duplicate addresses are rejected
2. Holding registers: duplicate register addresses are rejected
3. Missing required address attributes are rejected
4. Unsupported or malformed keys are rejected
5. Non-positive scale values are rejected

## Why this shape

The project now keeps a single holding-register derive path (`HoldingRegistersModel`) aligned with `modbus_app` routing and server request handling. This avoids parallel derive stacks with overlapping responsibilities.

## Usage Example

```rust
use mbus_server::{CoilsModel, HoldingRegistersModel, modbus_app};

#[derive(Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    run: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Holding {
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    temp: u16,
}

#[derive(Default)]
#[modbus_app(holding_registers(holding))]
struct App {
    holding: Holding,
}
```

## Convenience retained

After removing the typed parallel derive path, these conveniences remain:
1. Engineering-value helpers through `scale`
2. Unit metadata helper methods through `unit`
3. FC03/FC04/FC06/FC10 routing integration via `modbus_app`
