# Server Macros Reference

Derive macros and routing helpers for the synchronous server stack.

---

## Overview

The server-side macros split into two layers:

| Macro | Purpose |
|-------|---------|
| `CoilsModel` | Generate a `CoilMap` implementation for coil fields |
| `HoldingRegistersModel` | Generate a `HoldingRegisterMap` implementation |
| `InputRegistersModel` | Generate an `InputRegisterMap` implementation |
| `DiscreteInputsModel` | Generate a `DiscreteInputMap` implementation |
| `modbus_app` | Route one or more maps into the split server handler traits |
| `async_modbus_app` | Async server-side routing for the async adapters |

The generated code targets the current buffer-writing server traits rather than the older `Coils` or `Registers` return wrappers.

---

## `CoilsModel`

```rust
#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    run_enable: bool,
    #[coil(addr = 1, notify_via_batch = true)]
    alarm_reset: bool,
}
```

Supported field attributes:

| Attribute | Meaning |
|-----------|---------|
| `addr = N` | Required Modbus address |
| `notify_via_batch = true` | Route FC05 single writes to the group batch hook when no `on_write_N` hook is declared |

Generated trait shape:

```rust
impl CoilMap for Outputs {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;
    const HAS_BATCH_NOTIFIED_FIELDS: bool;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, MbusError>;
    fn write_single(&mut self, address: u16, value: bool) -> Result<(), MbusError>;
    fn write_many_from_packed(
        &mut self,
        address: u16,
        quantity: u16,
        values: &[u8],
        packed_bit_offset: usize,
    ) -> Result<(), MbusError>;
    fn is_batch_notified(addr: u16) -> bool;
}
```

---

## `HoldingRegistersModel`

```rust
#[derive(Default, HoldingRegistersModel)]
struct Setpoints {
    #[reg(addr = 0)]
    speed: u16,
    #[reg(addr = 1, scale = 10)]
    temp_tenths: u16,
    #[reg(addr = 2, unit = "RPM")]
    max_speed: u16,
}
```

Supported field attributes:

| Attribute | Meaning |
|-----------|---------|
| `addr = N` | Required Modbus address |
| `scale = ...` | Generate scaled getters and setters |
| `unit = "..."` | Generate a unit accessor |
| `notify_via_batch = true` | Route FC06 through the batch hook when no `on_write_N` hook is declared |

Generated API includes convenience getters and setters plus the routing trait:

```rust
impl HoldingRegisterMap for Setpoints {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;
    const HAS_BATCH_NOTIFIED_FIELDS: bool;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, MbusError>;
    fn write_single(&mut self, address: u16, value: u16) -> Result<(), MbusError>;
    fn write_many(&mut self, address: u16, values: &[u16]) -> Result<(), MbusError>;
    fn is_batch_notified(addr: u16) -> bool;
}
```

---

## Read-Only Models

`InputRegistersModel` and `DiscreteInputsModel` only generate encoding support because the protocol treats those ranges as read-only.

```rust
impl InputRegisterMap for Sensors {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, MbusError>;
}

impl DiscreteInputMap for Status {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, MbusError>;
}
```

---

## `modbus_app`

`#[modbus_app]` wires one or more fields from your app struct into the split server traits.

```rust
#[modbus_app(
    coils(outputs),
    holding_registers(setpoints),
    input_registers(sensors),
    discrete_inputs(status),
    fifo(history),
    file_record(files),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
    sensors: Sensors,
    status: Status,
    history: HistoryFifo,
    files: FileBlocks,
}
```

Supported routing groups:

- `coils(...)`
- `holding_registers(...)`
- `input_registers(...)`
- `discrete_inputs(...)`
- `fifo(...)`
- `file_record(...)`

`fifo(...)` and `file_record(...)` are selector groups rather than address ranges:

- FC18 selects by `FifoQueue::POINTER_ADDRESS`
- FC14 and FC15 select by `FileRecord::FILE_NUMBER`

---

## Generated Handler Shape

The macro generates the split trait impls directly.

```rust
impl ServerCoilHandler for App {
    fn read_coils_request(...) -> Result<u8, MbusError>;
    fn write_single_coil_request(...) -> Result<(), MbusError>;
    fn write_multiple_coils_request(...) -> Result<(), MbusError>;
}

impl ServerHoldingRegisterHandler for App {
    fn read_multiple_holding_registers_request(...) -> Result<u8, MbusError>;
    fn write_single_register_request(...) -> Result<(), MbusError>;
    fn write_multiple_registers_request(...) -> Result<(), MbusError>;
    fn mask_write_register_request(...) -> Result<(), MbusError>;
    fn read_write_multiple_registers_request(...) -> Result<u8, MbusError>;
}
```

`ServerExceptionHandler` remains the usual default no-op unless you override it yourself.

---

## Hook Parameters

Hook configuration is declared inside the routed group:

```rust
#[modbus_app(
    coils(outputs, on_write_0 = on_run_enable_changed, on_batch_write = on_outputs_written),
    holding_registers(setpoints, on_write_1 = on_temp_changed),
)]
```

Supported hook declarations:

| Parameter | Applies to | Meaning |
|-----------|------------|---------|
| `on_write_N = fn_name` | `coils`, `holding_registers` | Per-address hook for FC05 or FC06 |
| `on_batch_write = fn_name` | `coils`, `holding_registers` | Batch hook for FC0F, FC10, and the write half of FC17; also for FC05 and FC06 when `notify_via_batch = true` applies |

For hook ordering and commit behavior, see [Write Hooks](write_hooks.md).

---

## Compile-Time Validation

The macros reject several invalid configurations at compile time:

- duplicate addresses inside a map
- unsupported field types
- missing `addr = ...`
- invalid `scale` usage
- `on_write_N` pointing at an unmapped address
- `notify_via_batch = true` without an `on_batch_write` hook on that routed group
- overlapping routed maps where the macro cannot disambiguate ownership

---

## Minimal Example

```rust
use modbus_rs::{modbus_app, CoilsModel, HoldingRegistersModel};

#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    pump_enable: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Registers {
    #[reg(addr = 0)]
    setpoint: u16,
}

#[derive(Default)]
#[modbus_app(
    coils(outputs),
    holding_registers(registers),
)]
struct App {
    outputs: Outputs,
    registers: Registers,
}
```

---

## See Also

- [Building Applications](building_applications.md)
- [Write Hooks](write_hooks.md)
- [Function Codes](function_codes.md)
