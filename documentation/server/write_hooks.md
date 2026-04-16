# Server Write Hooks

React to client writes with per-field and batch hooks.

---

## Overview

Write hooks allow your application to react when Modbus clients write to coils or registers:

| Hook Type | Trigger | Use Case |
|-----------|---------|----------|
| **Per-field** | Single address written | Validation, side effects |
| **Batch** | Any write to map | Bulk processing, persistence |

---

## Per-Field Hooks

Declare per-field hooks in the `#[modbus_app(...)]` attribute with `on_write_N = method_name`.

**Two ways to use per-field hooks:**

1. **Explicit declaration** (recommended): Use `on_write_N = method_name` in the attribute
2. **Routing via batch**: Use `notify_via_batch = true` on the field to route FC05/FC06 single writes to the batch hook instead

### Per-Field Hook Signature

```rust
fn on_write_N(&mut self, address: u16, old_value: T, new_value: T) -> Result<(), MbusError>
```

- `address` — The Modbus address that was written (matches `N`)
- `old_value` — Previous value (bool for coils, u16 for registers)
- `new_value` — New value being written
- Return `Ok(())` to accept the write, or an `MbusError` to reject it

**Note:** If a hook returns an error, the write is **still applied** to the data model, but an exception response is sent to the client.

### Coil Hook

```rust
#[derive(Default, CoilsModel)]
struct MyCoils {
    #[coil(addr = 0)]
    motor_enable: bool,
    #[coil(addr = 1)]
    heater_enable: bool,
}

#[modbus_app(
    coils(coils, on_write_0 = on_write_0, on_write_1 = on_write_1),
)]
struct App {
    coils: MyCoils,
}

impl App {
    /// Called when coil at address 0 is written
    fn on_write_0(&mut self, address: u16, old_value: bool, new_value: bool) -> Result<(), MbusError> {
        if !old_value && new_value {
            println!("Motor starting");
            self.start_motor();
        } else if old_value && !new_value {
            println!("Motor stopping");
            self.stop_motor();
        }
        Ok(())
    }
    
    /// Called when coil at address 1 is written
    fn on_write_1(&mut self, address: u16, _old: bool, new: bool) -> Result<(), MbusError> {
        if new {
            self.enable_heater();
        } else {
            self.disable_heater();
        }
        Ok(())
    }
}
```

### Register Hook

```rust
#[derive(Default, HoldingRegistersModel)]
struct MyRegisters {
    #[reg(addr = 0)]
    setpoint: u16,
    #[reg(addr = 1, scale = 10)]
    temperature: u16,
}

#[modbus_app(
    holding_registers(registers, on_write_0 = on_write_0, on_write_1 = on_write_1),
)]
struct App {
    registers: MyRegisters,
}

impl App {
    /// Called when register at address 0 is written
    fn on_write_0(&mut self, address: u16, old_value: u16, new_value: u16) -> Result<(), MbusError> {
        println!("Setpoint changed: {} → {}", old_value, new_value);
        self.apply_setpoint(new_value);
        Ok(())
    }
    
    /// Called when register at address 1 is written
    fn on_write_1(&mut self, address: u16, old_value: u16, new_value: u16) -> Result<(), MbusError> {
        // Temperature scaled by 10
        let old_temp = old_value as f32 / 10.0;
        let new_temp = new_value as f32 / 10.0;
        println!("Temperature setpoint: {:.1}°C → {:.1}°C", old_temp, new_temp);
        Ok(())
    }
}
```

---

## Alternative: Using `notify_via_batch` on Fields

Instead of declaring per-field hooks in the attribute, you can use `notify_via_batch = true` on individual fields to route their writes to the batch hook:

```rust
#[derive(Default, HoldingRegistersModel)]
struct MyRegisters {
    #[reg(addr = 0, notify_via_batch = true)]
    critical_setting: u16,  // FC06 routes to batch hook
    
    #[reg(addr = 1)]
    normal_field: u16,  // FC06 goes to per-field hook (if declared)
}

#[modbus_app(
    holding_registers(registers, on_batch_write = on_registers_changed),
)]
struct App {
    registers: MyRegisters,
}

impl App {
    fn on_registers_changed(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError> {
        // Handles all writes, including single-register writes with notify_via_batch
        println!("Registers written: {} values starting at {}", values.len(), start);
        Ok(())
    }
}
```

---

## Batch Hooks

Define a batch hook to handle all writes to a data model at once.

### Batch Hook Signature

Batch hooks receive the actual data being written:

**For coils (FC0F):**
```rust
fn on_batch_write(&mut self, address: u16, quantity: u16, packed_bits: &[u8]) -> Result<(), MbusError>
```

**For holding registers (FC10):**
```rust
fn on_batch_write(&mut self, address: u16, quantity: u16, values: &[u16]) -> Result<(), MbusError>
```

- `address` — First Modbus address written
- `quantity` — Number of addresses written (1 or more)
- `packed_bits` or `values` — The actual data being written (u8 bytes for coils, u16 words for registers)
- Return `Ok(())` to accept the writes, or an `MbusError` to reject them

**Note:** Like per-field hooks, if a batch hook returns an error, the write is **still applied** to the data model, but an exception response is sent to the client.

### Syntax

```rust
#[modbus_app(
    coils(coils, on_batch_write = on_coils_changed),
    holding_registers(registers, on_batch_write = on_registers_changed),
)]
struct App {
    coils: MyCoils,
    registers: MyRegisters,
}

impl App {
    fn on_coils_changed(&mut self, start_address: u16, quantity: u16, packed_bits: &[u8]) -> Result<(), MbusError> {
        println!("Wrote {} coils at address {} with {} bytes", quantity, start_address, packed_bits.len());
        self.persist_coils();
        Ok(())
    }
    
    fn on_registers_changed(&mut self, start_address: u16, quantity: u16, values: &[u16]) -> Result<(), MbusError> {
        println!("Wrote {} registers at address {} with values: {:?}", quantity, start_address, values);
        self.validate_and_apply();
        Ok(())
    }
}
```

### Batch Hook Signature

```rust
fn hook_name(&mut self, start_address: u16, quantity: u16)
```

- `start_address` — First address written
- `quantity` — Number of addresses written

---

## Combining Per-Field and Batch Hooks

Both hook types can be used together by declaring both in the attribute:

```rust
#[modbus_app(
    holding_registers(registers, on_write_0 = on_write_0, on_batch_write = on_any_write),
)]
struct App {
    registers: MyRegisters,
}

impl App {
    /// Called for any write (batch)
    fn on_any_write(&mut self, start: u16, qty: u16) -> Result<(), MbusError> {
        self.dirty = true;
        self.last_write_time = Instant::now();
        Ok(())
    }
    
    /// Called only for address 0 (per-field)
    fn on_write_0(&mut self, address: u16, _old: u16, new: u16) -> Result<(), MbusError> {
        self.apply_critical_setting(new);
        Ok(())
    }
}
```

Execution order:
1. Per-field hooks are called first (for each address in the write)
2. Batch hook is called after all per-field hooks

---

## Single-Write Routing via `notify_via_batch`

By default, FC05 (Write Single Coil) and FC06 (Write Single Register) call per-field hooks.

Use `notify_via_batch = true` to route single writes through the batch hook instead:

```rust
#[derive(Default, HoldingRegistersModel)]
struct MyRegisters {
    #[reg(addr = 0)]
    normal_field: u16,
    
    #[reg(addr = 1, notify_via_batch = true)]
    batch_routed_field: u16,  // FC06 goes to batch hook
}
```

This is useful when:
- You want consistent handling for single and multiple writes
- The batch hook contains the main processing logic

---

## Validation in Write Hooks

Reject invalid writes by returning an error:

```rust
impl App {
    fn on_write_0(&mut self, _addr: u16, _old: u16, new: u16) -> Result<(), MbusError> {
        // Validate range
        if new > 1000 {
            return Err(MbusError::InvalidData);
        }
        
        // Check device state
        if self.is_running && new != 0 {
            return Err(MbusError::DeviceBusy);
        }
        
        self.apply_setting(new);
        Ok(())
    }
}
```

**Note:** If a hook returns an error, the write is still applied to the data model, but an exception response is sent to the client.

---

## Side Effects in Hooks

Common patterns for side effects:

### Hardware Control

```rust
fn on_write_0(&mut self, _addr: u16, _old: bool, new: bool) -> Result<(), MbusError> {
    if new {
        self.gpio.set_high(MOTOR_PIN);
    } else {
        self.gpio.set_low(MOTOR_PIN);
    }
    Ok(())
}
```

### Logging

```rust
fn on_write_0(&mut self, _addr: u16, old: u16, new: u16) -> Result<(), MbusError> {
    info!(
        "Register 0 changed: {} → {} by unit {}",
        old, new, self.last_unit_id
    );
    Ok(())
}
```

### Persistence

```rust
fn on_registers_changed(&mut self, _start: u16, _qty: u16) -> Result<(), MbusError> {
    self.needs_save = true;
    Ok(())
}

// In poll loop:
if app.needs_save && app.last_write.elapsed() > Duration::from_secs(1) {
    app.save_to_eeprom();
    app.needs_save = false;
}
```

### Derived Values

```rust
fn on_write_0(&mut self, _addr: u16, _old: u16, new: u16) -> Result<(), MbusError> {
    // Update setpoint
    self.registers.setpoint = new;
    
    // Recalculate derived values
    self.pid_controller.set_target(new);
    self.update_status_flags();
    Ok(())
}
```

---

## Complete Example

```rust
use modbus_rs::{modbus_app, CoilsModel, HoldingRegistersModel, MbusError};

#[derive(Default, CoilsModel)]
struct Outputs {
    #[coil(addr = 0)]
    motor_enable: bool,
    #[coil(addr = 1, notify_via_batch = true)]
    alarm_ack: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct Setpoints {
    #[reg(addr = 0)]
    speed_setpoint: u16,
    #[reg(addr = 1, scale = 10)]
    temp_setpoint: u16,
}

#[modbus_app(
    coils(outputs, on_write_0 = on_write_0, on_batch_write = on_outputs_changed),
    holding_registers(setpoints),
)]
struct App {
    outputs: Outputs,
    setpoints: Setpoints,
    motor_running: bool,
    alarm_active: bool,
}

impl App {
    // Per-field hook for motor enable
    fn on_write_0(&mut self, address: u16, old: bool, new: bool) -> Result<(), MbusError> {
        if new && self.alarm_active {
            return Err(MbusError::DeviceBusy);
        }
        
        if !old && new {
            println!("Starting motor");
            self.motor_running = true;
        } else if old && !new {
            println!("Stopping motor");
            self.motor_running = false;
        }
        
        Ok(())
    }
    
    // Batch hook for all output changes
    fn on_outputs_changed(&mut self, start: u16, qty: u16, packed_bits: &[u8]) -> Result<(), MbusError> {
        println!("Outputs written: {} coils at {}", qty, start);
        
        // Check if alarm ack was set
        if self.outputs.alarm_ack {
            self.alarm_active = false;
            self.outputs.alarm_ack = false;  // Auto-reset
        }
        Ok(())
    }
}
```

---

## See Also

- [Macros](macros.md) — Derive macro reference
- [Building Applications](building_applications.md) — Full guide
- [Function Codes](function_codes.md) — FC details
