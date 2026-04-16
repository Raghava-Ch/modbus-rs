# Server Macros Reference

Derive macros for declarative Modbus data models.

---

## Overview

The derive macros generate compile-time mappings between Rust structs and Modbus protocol memory:

| Macro | Function Codes | Direction |
|-------|----------------|-----------|
| `CoilsModel` | FC01, FC05, FC0F | Read/Write |
| `HoldingRegistersModel` | FC03, FC06, FC10, FC16, FC17 | Read/Write |
| `InputRegistersModel` | FC04 | Read-only |
| `DiscreteInputsModel` | FC02 | Read-only |
| `modbus_app` | all above | Routing + Validation |

---

## `CoilsModel`

Maps boolean fields to Modbus coils.

### Syntax

```rust
#[derive(Default, CoilsModel)]
struct MyCoils {
    #[coil(addr = 0)]
    output_enable: bool,
    
    #[coil(addr = 1)]
    heater_on: bool,
    
    #[coil(addr = 2, notify_via_batch = true)]
    alarm_ack: bool,
}
```

### Attributes

| Attribute | Required | Description |
|-----------|----------|-------------|
| `addr = <u16>` | ✅ | Modbus coil address |
| `notify_via_batch = true` | ❌ | Route FC05 single writes to batch hook |

### Generated Trait

```rust
impl CoilMap for MyCoils {
    fn encode(&self, range: Range<u16>) -> Coils;
    fn decode(&mut self, coils: &Coils);
    fn addresses() -> &'static [u16];
}
```

---

## `HoldingRegistersModel`

Maps u16 fields to Modbus holding registers.

### Syntax

```rust
#[derive(Default, HoldingRegistersModel)]
struct MyRegisters {
    #[reg(addr = 0)]
    setpoint: u16,
    
    #[reg(addr = 1, scale = 10)]
    temperature: u16,  // 0.1°C resolution
    
    #[reg(addr = 2, unit = "RPM")]
    speed: u16,
    
    #[reg(addr = 3, notify_via_batch = true)]
    config_value: u16,
}
```

### Attributes

| Attribute | Required | Description |
|-----------|----------|-------------|
| `addr = <u16>` | ✅ | Modbus register address |
| `scale = <number>` | ❌ | Scaling factor (must be > 0) |
| `unit = "..."` | ❌ | Unit string for documentation |
| `notify_via_batch = true` | ❌ | Route FC06 single writes to batch hook |

### Generated Methods

```rust
impl MyRegisters {
    // Basic getter/setter
    fn get_temperature(&self) -> u16;
    fn set_temperature(&mut self, value: u16);
    
    // When scale is present
    fn get_temperature_scaled(&self) -> f32;
    fn set_temperature_scaled(&mut self, value: f32);
    
    // When unit is present
    fn get_speed_unit() -> &'static str;
}
```

### Generated Trait

```rust
impl HoldingRegisterMap for MyRegisters {
    fn encode(&self, range: Range<u16>) -> Registers;
    fn decode(&mut self, registers: &Registers);
    fn addresses() -> &'static [u16];
}
```

---

## `InputRegistersModel`

Maps u16 fields to Modbus input registers (read-only from Modbus perspective).

### Syntax

```rust
#[derive(Default, InputRegistersModel)]
struct MySensors {
    #[reg(addr = 0, scale = 10)]
    temperature: u16,
    
    #[reg(addr = 1)]
    pressure: u16,
}
```

Input registers use the same attributes as holding registers.

### Generated Trait

```rust
impl InputRegisterMap for MySensors {
    fn encode(&self, range: Range<u16>) -> Registers;
    fn addresses() -> &'static [u16];
}
```

**Note:** No `decode()` method — input registers are read-only from the Modbus client's perspective.

---

## `DiscreteInputsModel`

Maps boolean fields to Modbus discrete inputs (read-only).

### Syntax

```rust
#[derive(Default, DiscreteInputsModel)]
struct MyStatus {
    #[discrete_input(addr = 0)]
    motor_running: bool,
    
    #[discrete_input(addr = 1)]
    door_open: bool,
}
```

### Attributes

| Attribute | Required | Description |
|-----------|----------|-------------|
| `addr = <u16>` | ✅ | Modbus discrete input address |

### Generated Trait

```rust
impl DiscreteInputMap for MyStatus {
    fn encode(&self, range: Range<u16>) -> DiscreteInputs;
    fn addresses() -> &'static [u16];
}
```

---

## `modbus_app`

Combines data models into a complete `ModbusAppHandler` implementation.

### Syntax

```rust
#[modbus_app(
    coils(coils),
    holding_registers(registers),
    input_registers(sensors),
    discrete_inputs(status),
)]
struct App {
    coils: MyCoils,
    registers: MyRegisters,
    sensors: MySensors,
    status: MyStatus,
}
```

Each argument inside a group must be the field name(s) of the corresponding data model in the struct.

### Optional Hook Parameters

| Parameter | Applies to | Description |
|-----------|------------|-------------|
| `on_batch_write = fn_name` | `coils`, `holding_registers` | Called after any multi-write (FC10, FC0F) |
| `on_write_N = fn_name` | `coils`, `holding_registers` | Called after a single-address write to address `N` (FC05, FC06) |

### Generated Implementation

```rust
impl ModbusAppHandler for App {
    fn read_coils_request(...) -> Result<Coils, MbusError>;
    fn write_single_coil_request(...) -> Result<(), MbusError>;
    fn write_multiple_coils_request(...) -> Result<(), MbusError>;
    fn read_multiple_holding_registers_request(...) -> Result<Registers, MbusError>;
    // ... all applicable callbacks
}
```

---

## Write Hooks

React to writes with per-field or batch hooks.

### Per-Field Hook

```rust
#[modbus_app(
    holding_registers(registers, on_write_1 = on_setpoint_changed),
)]
struct App {
    registers: MyRegisters,
}

impl App {
    /// Called when register at address 1 is written
    fn on_setpoint_changed(&mut self, old_value: u16, new_value: u16) {
        println!("Setpoint changed: {} → {}", old_value, new_value);
    }
}
```

### Batch Hook

```rust
#[modbus_app(
    coils(coils, on_batch_write = on_coils_batch),
)]
struct App {
    coils: MyCoils,
}

impl App {
    fn on_coils_batch(&mut self, start_address: u16, quantity: u16) {
        println!("Coils written: {} starting at {}", quantity, start_address);
    }
}
```

See [Write Hooks](write_hooks.md) for complete details.

---

## Validation Rules

The macros enforce compile-time validation:

| Rule | Error |
|------|-------|
| Duplicate coil addresses | Compile error |
| Duplicate register addresses | Compile error |
| Missing `addr` attribute | Compile error |
| Invalid field type (not `bool` for coils) | Compile error |
| Invalid field type (not `u16` for registers) | Compile error |
| Non-positive scale value | Compile error |
| Overlapping ranges in `modbus_app` | Compile error |
| `on_write_N` targets unmapped address | Compile error |

---

## Complete Example

```rust
use modbus_rs::{modbus_app, CoilsModel, HoldingRegistersModel, InputRegistersModel, DiscreteInputsModel};

#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    motor_enable: bool,
    #[coil(addr = 1)]
    heater_enable: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Setpoints {
    #[reg(addr = 0)]
    speed_setpoint: u16,
    #[reg(addr = 1, scale = 10)]
    temp_setpoint: u16,
}

#[derive(Default, InputRegistersModel)]
struct Sensors {
    #[reg(addr = 0)]
    actual_speed: u16,
    #[reg(addr = 1, scale = 10)]
    actual_temp: u16,
}

#[derive(Default, DiscreteInputsModel)]
struct Status {
    #[discrete_input(addr = 0)]
    motor_running: bool,
    #[discrete_input(addr = 1)]
    temp_alarm: bool,
}

#[modbus_app(
    coils(outputs),
    holding_registers(setpoints),
    input_registers(sensors),
    discrete_inputs(status),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
    sensors: Sensors,
    status: Status,
}

impl App {
    fn on_write_0(&mut self, _old: bool, new: bool) {
        println!("Motor enable: {}", new);
    }
}
```

---

## See Also

- [Building Applications](building_applications.md)
- [Write Hooks](write_hooks.md)
- [Function Codes](function_codes.md)
