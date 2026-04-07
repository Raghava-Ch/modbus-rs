#[cfg(feature = "registers")]
use crate::c::MbusStatusCode;
use mbus_core::models::register::Registers;

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
/// Reads a single register value by 0-based index into `out_value`.
///
/// `index` must be less than [`mbus_registers_quantity`]; otherwise returns
/// `MBUS_ERR_INVALID_ADDRESS`. Unlike [`mbus_registers_value`], no knowledge of
/// the Modbus starting address is required.
pub unsafe extern "C" fn mbus_registers_value_at_index(
    registers: *const MbusRegisters,
    index: u16,
    out_value: *mut u16,
) -> MbusStatusCode {
    if registers.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    let inner = unsafe { (*registers).inner() };
    let address = inner.from_address().saturating_add(index);
    match inner.value(address) {
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

#[cfg(test)]
#[cfg(feature = "registers")]
mod tests {
    use super::*;
    use mbus_core::models::register::Registers;

    fn make_registers(from_address: u16, quantity: u16) -> MbusRegisters {
        // Build a Registers instance where register[i] = (from_address + i)
        MbusRegisters(
            Registers::new(from_address, quantity)
                .unwrap()
                .with_values(
                    &(0..quantity)
                        .map(|i| from_address + i)
                        .collect::<heapless::Vec<_, 125>>(),
                    quantity,
                )
                .unwrap(),
        )
    }

    // ── Null-pointer guards ───────────────────────────────────────────────────

    #[test]
    fn from_address_null_returns_zero() {
        assert_eq!(unsafe { mbus_registers_from_address(core::ptr::null()) }, 0);
    }

    #[test]
    fn quantity_null_returns_zero() {
        assert_eq!(unsafe { mbus_registers_quantity(core::ptr::null()) }, 0);
    }

    #[test]
    fn value_null_registers_returns_null_pointer_error() {
        let mut out: u16 = 0;
        let rc = unsafe { mbus_registers_value(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_null_out_returns_null_pointer_error() {
        let r = make_registers(0, 4);
        let rc = unsafe { mbus_registers_value(&r, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_registers_returns_null_pointer_error() {
        let mut out: u16 = 0;
        let rc = unsafe { mbus_registers_value_at_index(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_out_returns_null_pointer_error() {
        let r = make_registers(0, 4);
        let rc = unsafe { mbus_registers_value_at_index(&r, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn values_ptr_null_returns_null() {
        assert!(unsafe { mbus_registers_values_ptr(core::ptr::null()) }.is_null());
    }

    // ── Correct values ────────────────────────────────────────────────────────

    #[test]
    fn from_address_and_quantity_round_trip() {
        let r = make_registers(100, 4);
        assert_eq!(unsafe { mbus_registers_from_address(&r) }, 100);
        assert_eq!(unsafe { mbus_registers_quantity(&r) }, 4);
    }

    #[test]
    fn value_reads_correct_register_by_address() {
        // register at address 5 was loaded with value 5 (from_address + 0)
        let r = make_registers(5, 4);
        let mut out: u16 = 0;
        assert_eq!(
            unsafe { mbus_registers_value(&r, 5, &mut out) },
            MbusStatusCode::MbusOk
        );
        assert_eq!(out, 5);
        assert_eq!(
            unsafe { mbus_registers_value(&r, 6, &mut out) },
            MbusStatusCode::MbusOk
        );
        assert_eq!(out, 6);
    }

    #[test]
    fn value_at_index_is_address_independent() {
        // High base address — index 0 should still give the first value
        let r = make_registers(5000, 3);
        let mut out: u16 = 0;
        assert_eq!(
            unsafe { mbus_registers_value_at_index(&r, 0, &mut out) },
            MbusStatusCode::MbusOk
        );
        assert_eq!(out, 5000);
        assert_eq!(
            unsafe { mbus_registers_value_at_index(&r, 2, &mut out) },
            MbusStatusCode::MbusOk
        );
        assert_eq!(out, 5002);
    }

    #[test]
    fn value_out_of_range_returns_invalid_address() {
        let r = make_registers(0, 4); // valid 0..3
        let mut out: u16 = 0;
        let rc = unsafe { mbus_registers_value(&r, 4, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrInvalidAddress);
    }

    #[test]
    fn value_at_index_out_of_range_returns_invalid_address() {
        let r = make_registers(0, 4); // valid indices 0..3
        let mut out: u16 = 0;
        let rc = unsafe { mbus_registers_value_at_index(&r, 4, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrInvalidAddress);
    }
}
