#[cfg(feature = "coils")]
use crate::c::MbusStatusCode;
use mbus_core::models::coil::Coils;

// ── Opaque Handle ─────────────────────────────────────────────────────────────

/// Opaque handle to a Coils instance (Rust-owned memory).
#[repr(C)]
pub struct MbusCoils(pub(crate) Coils);

#[cfg(feature = "coils")]
impl MbusCoils {
    pub(in crate::c) fn inner(&self) -> &Coils {
        &self.0
    }
}

// ── C API Functions ──────────────────────────────────────────────────────────

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Returns the starting address of the coils range.
///
/// # Safety
/// `coils` must either be null (returns 0) or point to a valid `MbusCoils`.
pub unsafe extern "C" fn mbus_coils_from_address(coils: *const MbusCoils) -> u16 {
    if coils.is_null() {
        return 0;
    }
    unsafe { (*coils).inner().from_address() }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Returns the number of coils.
///
/// # Safety
/// `coils` must either be null (returns 0) or point to a valid `MbusCoils`.
pub unsafe extern "C" fn mbus_coils_quantity(coils: *const MbusCoils) -> u16 {
    if coils.is_null() {
        return 0;
    }
    unsafe { (*coils).inner().quantity() }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Reads a single coil value by address into `out_value`.
///
/// # Safety
/// `coils` and `out_value` must be non-null and point to valid memory, or null
/// (returns `MBUS_ERR_NULL_POINTER` for null).
pub unsafe extern "C" fn mbus_coils_value(
    coils: *const MbusCoils,
    address: u16,
    out_value: *mut bool,
) -> MbusStatusCode {
    if coils.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }

    match unsafe { (*coils).inner().value(address) } {
        Ok(value) => {
            unsafe { *out_value = value };
            MbusStatusCode::MbusOk
        }
        Err(e) => MbusStatusCode::from(e),
    }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Reads a single coil value by 0-based index into `out_value`.
///
/// `index` must be less than [`mbus_coils_quantity`]; otherwise returns
/// `MBUS_ERR_INVALID_ADDRESS`. Unlike [`mbus_coils_value`], no knowledge of
/// the Modbus starting address is required.
///
/// # Safety
/// `coils` and `out_value` must be non-null and point to valid memory, or null
/// (returns `MBUS_ERR_NULL_POINTER` for null).
pub unsafe extern "C" fn mbus_coils_value_at_index(
    coils: *const MbusCoils,
    index: u16,
    out_value: *mut bool,
) -> MbusStatusCode {
    if coils.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    let inner = unsafe { (*coils).inner() };
    let address = inner.from_address().saturating_add(index);
    match inner.value(address) {
        Ok(value) => {
            unsafe { *out_value = value };
            MbusStatusCode::MbusOk
        }
        Err(e) => MbusStatusCode::from(e),
    }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Returns a raw pointer to the packed coil bit-values. Valid during callback only.
///
/// # Safety
/// `coils` must either be null (returns a null pointer) or point to a valid `MbusCoils`.
pub unsafe extern "C" fn mbus_coils_values_ptr(coils: *const MbusCoils) -> *const u8 {
    if coils.is_null() {
        return core::ptr::null();
    }
    unsafe { (*coils).inner().values().as_ptr() }
}

#[cfg(test)]
#[cfg(feature = "coils")]
mod tests {
    use super::*;
    use mbus_core::models::coil::Coils;

    fn make_coils(from_address: u16, quantity: u16, byte0: u8) -> MbusCoils {
        let mut vals = [0u8; 250]; // MAX_COIL_BYTES
        vals[0] = byte0;
        MbusCoils(
            Coils::new(from_address, quantity)
                .unwrap()
                .with_values(&vals, quantity)
                .unwrap(),
        )
    }

    // ── Null-pointer guards ───────────────────────────────────────────────────

    #[test]
    fn from_address_null_returns_zero() {
        let v = unsafe { mbus_coils_from_address(core::ptr::null()) };
        assert_eq!(v, 0);
    }

    #[test]
    fn quantity_null_returns_zero() {
        let v = unsafe { mbus_coils_quantity(core::ptr::null()) };
        assert_eq!(v, 0);
    }

    #[test]
    fn value_null_coils_returns_null_pointer_error() {
        let mut out = false;
        let rc = unsafe { mbus_coils_value(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_null_out_returns_null_pointer_error() {
        let c = make_coils(0, 8, 0x01);
        let rc = unsafe { mbus_coils_value(&c, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_coils_returns_null_pointer_error() {
        let mut out = false;
        let rc = unsafe { mbus_coils_value_at_index(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_out_returns_null_pointer_error() {
        let c = make_coils(0, 8, 0x01);
        let rc = unsafe { mbus_coils_value_at_index(&c, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn values_ptr_null_returns_null() {
        let p = unsafe { mbus_coils_values_ptr(core::ptr::null()) };
        assert!(p.is_null());
    }

    // ── Correct values ────────────────────────────────────────────────────────

    #[test]
    fn from_address_and_quantity_round_trip() {
        // 8 coils starting at address 100
        let c = make_coils(100, 8, 0x00);
        assert_eq!(unsafe { mbus_coils_from_address(&c) }, 100);
        assert_eq!(unsafe { mbus_coils_quantity(&c) }, 8);
    }

    #[test]
    fn value_reads_correct_bit_by_address() {
        // byte0 = 0b0000_0101 → bit 0 (addr 10) ON, bit 1 (addr 11) OFF, bit 2 (addr 12) ON
        let c = make_coils(10, 8, 0b0000_0101);
        let mut v = false;
        assert_eq!(
            unsafe { mbus_coils_value(&c, 10, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "addr 10 should be ON");
        assert_eq!(
            unsafe { mbus_coils_value(&c, 11, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(!v, "addr 11 should be OFF");
        assert_eq!(
            unsafe { mbus_coils_value(&c, 12, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "addr 12 should be ON");
    }

    #[test]
    fn value_at_index_is_address_independent() {
        // Same data as above but queried by index, not absolute address
        let c = make_coils(10, 8, 0b0000_0101);
        let mut v = false;
        assert_eq!(
            unsafe { mbus_coils_value_at_index(&c, 0, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "index 0 should be ON");
        assert_eq!(
            unsafe { mbus_coils_value_at_index(&c, 1, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(!v, "index 1 should be OFF");

        // Non-zero base address: same index still accesses the same bit
        let c2 = make_coils(5000, 8, 0b0000_0101);
        assert_eq!(
            unsafe { mbus_coils_value_at_index(&c2, 0, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "index 0 at high base address should be ON");
    }

    #[test]
    fn value_out_of_range_returns_invalid_address() {
        let c = make_coils(10, 8, 0x00); // valid addresses 10..17
        let mut v = false;
        let rc = unsafe { mbus_coils_value(&c, 18, &mut v) }; // address 18 is OOB
        assert_eq!(rc, MbusStatusCode::MbusErrInvalidAddress);
    }

    #[test]
    fn value_at_index_out_of_range_returns_invalid_address() {
        let c = make_coils(10, 8, 0x00); // 8 coils → valid indices 0..7
        let mut v = false;
        let rc = unsafe { mbus_coils_value_at_index(&c, 8, &mut v) }; // index 8 is OOB
        assert_eq!(rc, MbusStatusCode::MbusErrInvalidAddress);
    }

    #[test]
    fn values_ptr_points_to_correct_byte() {
        let c = make_coils(0, 8, 0xAB);
        let p = unsafe { mbus_coils_values_ptr(&c) };
        assert!(!p.is_null());
        assert_eq!(unsafe { *p }, 0xAB);
    }
}
