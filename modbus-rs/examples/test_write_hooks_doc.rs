//! Test that write_hooks documentation examples compile correctly.
//!
//! Run: cargo run -p modbus-rs --example test_write_hooks_doc --features server

use mbus_core::errors::MbusError;
use mbus_server::{modbus_app, CoilsModel, HoldingRegistersModel};

#[derive(Default, CoilsModel)]
struct MyCoils {
    #[coil(addr = 0)]
    motor_enable: bool,
    #[coil(addr = 1)]
    heater_enable: bool,
}

#[derive(Default, HoldingRegistersModel)] 
struct MyRegisters {
    #[reg(addr = 0)]
    setpoint: u16,
    #[reg(addr = 1, scale = 10)]
    temperature: u16,
}

// Example from write_hooks.md - Per-Field Hooks section
#[derive(Default)]
#[modbus_app(
    coils(coils, on_write_0 = on_write_0, on_write_1 = on_write_1),
)]
struct App1 {
    coils: MyCoils,
}

impl App1 {
    fn on_write_0(&mut self, address: u16, old_value: bool, new_value: bool) -> Result<(), MbusError> {
        if !old_value && new_value {
            println!("Motor starting (addr: {})", address);
        }
        Ok(())
    }
    
    fn on_write_1(&mut self, address: u16, _old: bool, new: bool) -> Result<(), MbusError> {
        if new {
            println!("Heater enabled (addr: {})", address);
        }
        Ok(())
    }
}

// Example from write_hooks.md - Register Hook section
#[derive(Default)]
#[modbus_app(
    holding_registers(registers, on_write_0 = on_write_0, on_write_1 = on_write_1),
)]
struct App2 {
    registers: MyRegisters,
}

impl App2 {
    fn on_write_0(&mut self, address: u16, old_value: u16, new_value: u16) -> Result<(), MbusError> {
        println!("Setpoint changed (addr {}): {} → {}", address, old_value, new_value);
        Ok(())
    }
    
    fn on_write_1(&mut self, address: u16, old_value: u16, new_value: u16) -> Result<(), MbusError> {
        let old_temp = old_value as f32 / 10.0;
        let new_temp = new_value as f32 / 10.0;
        println!("Temperature setpoint (addr {}): {:.1}°C → {:.1}°C", address, old_temp, new_temp);
        Ok(())
    }
}

// Example from write_hooks.md - Combining Per-Field and Batch Hooks section
#[derive(Default)]
#[modbus_app(
    holding_registers(registers, on_write_0 = on_critical, on_batch_write = on_batch),
)]
struct App3 {
    registers: MyRegisters,
}

impl App3 {
    fn on_batch(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError> {
        println!("Batch write at addr {}, qty {}, values: {:?}", start, qty, values);
        Ok(())
    }
    
    fn on_critical(&mut self, address: u16, _old: u16, new: u16) -> Result<(), MbusError> {
        println!("Per-field: Address {} written with value {}", address, new);
        Ok(())
    }
}

fn main() {
    println!("✓ All write_hooks.md documentation examples compiled successfully!");
    
    // Quick sanity check that the apps can be instantiated
    let _app1 = App1::default();
    let _app2 = App2::default();
    let _app3 = App3::default();
    
    println!("✓ All apps instantiated successfully!");
    println!("✓ All hook signatures verified!");
}
