use mbus_core::models::register::Registers;
#[cfg(feature = "registers")]
use crate::c::MbusStatusCode;

// ── Opaque Handle ─────────────────────────────────────────────────────────────

/// Opaque handle to a Registers instance (Rust-owned memory).
#[repr(C)]
pub struct MbusRegisters(pub(crate) Registers);

impl MbusRegisters {
    #[cfg(feature = "registers")]
    pub(in crate::c) fn inner(&self) -> &Registers {
        &self.0
    }

    #[allow(dead_code)]
    pub(in crate::c) fn new(value: Registers) -> Self {
        Self(value)
    }
}

// ── C API Functions ──────────────────────────────────────────────────────────

#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
/// Returns the starting address of the registers range.
pub unsafe extern "C" fn mbus_registers_from_address(registers: *const MbusRegisters) -> u16 {
    if registers.is_null() {
        return 0;
    }
    unsafe { (*registers).inner().from_address() }
}

#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
/// Returns the number of registers.
pub unsafe extern "C" fn mbus_registers_quantity(registers: *const MbusRegisters) -> u16 {
    if registers.is_null() {
        return 0;
    }
    unsafe { (*registers).inner().quantity() }
}

#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
/// Reads a single register value by address into `out_value`.
pub unsafe extern "C" fn mbus_registers_value(
    registers: *const MbusRegisters,
    address: u16,
    out_value: *mut u16,
) -> MbusStatusCode {
    if registers.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }

    match unsafe { (*registers).inner().value(address) } {
        Ok(value) => {
            unsafe { *out_value = value };
            MbusStatusCode::MbusOk
        }
        Err(e) => MbusStatusCode::from(e),
    }
}

#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
/// Returns a raw pointer to the register values. Valid during callback only.
pub unsafe extern "C" fn mbus_registers_values_ptr(registers: *const MbusRegisters) -> *const u16 {
    if registers.is_null() {
        return core::ptr::null();
    }
    unsafe { (*registers).inner().values().as_ptr() }
}
