#[cfg(feature = "discrete-inputs")]
use crate::c::MbusStatusCode;
use mbus_core::models::discrete_input::DiscreteInputs;

// ── Opaque Handle ─────────────────────────────────────────────────────────────

/// Opaque handle to a DiscreteInputs instance (Rust-owned memory).
#[repr(C)]
pub struct MbusDiscreteInputs(pub(crate) DiscreteInputs);

impl MbusDiscreteInputs {
    #[cfg(feature = "discrete-inputs")]
    pub(in crate::c) fn inner(&self) -> &DiscreteInputs {
        &self.0
    }

    #[cfg(feature = "discrete-inputs")]
    pub(in crate::c) fn new(value: DiscreteInputs) -> Self {
        Self(value)
    }
}

// ── C API Functions ──────────────────────────────────────────────────────────

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Returns the starting address of the discrete inputs range.
pub unsafe extern "C" fn mbus_discrete_inputs_from_address(
    discrete_inputs: *const MbusDiscreteInputs,
) -> u16 {
    if discrete_inputs.is_null() {
        return 0;
    }
    unsafe { (*discrete_inputs).inner().from_address() }
}

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Returns the number of discrete inputs.
pub unsafe extern "C" fn mbus_discrete_inputs_quantity(
    discrete_inputs: *const MbusDiscreteInputs,
) -> u16 {
    if discrete_inputs.is_null() {
        return 0;
    }
    unsafe { (*discrete_inputs).inner().quantity() }
}

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Reads a single discrete input value by address into `out_value`.
pub unsafe extern "C" fn mbus_discrete_inputs_value(
    discrete_inputs: *const MbusDiscreteInputs,
    address: u16,
    out_value: *mut bool,
) -> MbusStatusCode {
    if discrete_inputs.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }

    match unsafe { (*discrete_inputs).inner().value(address) } {
        Ok(value) => {
            unsafe { *out_value = value };
            MbusStatusCode::MbusOk
        }
        Err(e) => MbusStatusCode::from(e),
    }
}

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Reads a single discrete input value by 0-based index into `out_value`.
///
/// `index` must be less than [`mbus_discrete_inputs_quantity`]; otherwise returns
/// `MBUS_ERR_INVALID_ADDRESS`. Unlike [`mbus_discrete_inputs_value`], no knowledge
/// of the Modbus starting address is required.
pub unsafe extern "C" fn mbus_discrete_inputs_value_at_index(
    discrete_inputs: *const MbusDiscreteInputs,
    index: u16,
    out_value: *mut bool,
) -> MbusStatusCode {
    if discrete_inputs.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    let inner = unsafe { (*discrete_inputs).inner() };
    let address = inner.from_address().saturating_add(index);
    match inner.value(address) {
        Ok(value) => {
            unsafe { *out_value = value };
            MbusStatusCode::MbusOk
        }
        Err(e) => MbusStatusCode::from(e),
    }
}

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Returns a raw pointer to the discrete input bit-values. Valid during callback only.
pub unsafe extern "C" fn mbus_discrete_inputs_values_ptr(
    discrete_inputs: *const MbusDiscreteInputs,
) -> *const u8 {
    if discrete_inputs.is_null() {
        return core::ptr::null();
    }
    unsafe { (*discrete_inputs).inner().values().as_ptr() }
}

#[cfg(test)]
#[cfg(feature = "discrete-inputs")]
mod tests {
    use super::*;
    use mbus_core::models::discrete_input::DiscreteInputs;

    fn make_discrete(from_address: u16, quantity: u16, byte0: u8) -> MbusDiscreteInputs {
        let mut vals = [0u8; 250];
        vals[0] = byte0;
        MbusDiscreteInputs(
            DiscreteInputs::new(from_address, quantity)
                .unwrap()
                .with_values(&vals, quantity)
                .unwrap(),
        )
    }

    // ── Null-pointer guards ───────────────────────────────────────────────────

    #[test]
    fn from_address_null_returns_zero() {
        assert_eq!(
            unsafe { mbus_discrete_inputs_from_address(core::ptr::null()) },
            0
        );
    }

    #[test]
    fn quantity_null_returns_zero() {
        assert_eq!(
            unsafe { mbus_discrete_inputs_quantity(core::ptr::null()) },
            0
        );
    }

    #[test]
    fn value_null_inputs_returns_null_pointer_error() {
        let mut out = false;
        let rc = unsafe { mbus_discrete_inputs_value(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_null_out_returns_null_pointer_error() {
        let d = make_discrete(0, 8, 0x01);
        let rc = unsafe { mbus_discrete_inputs_value(&d, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_inputs_returns_null_pointer_error() {
        let mut out = false;
        let rc = unsafe { mbus_discrete_inputs_value_at_index(core::ptr::null(), 0, &mut out) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn value_at_index_null_out_returns_null_pointer_error() {
        let d = make_discrete(0, 8, 0x01);
        let rc = unsafe { mbus_discrete_inputs_value_at_index(&d, 0, core::ptr::null_mut()) };
        assert_eq!(rc, MbusStatusCode::MbusErrNullPointer);
    }

    #[test]
    fn values_ptr_null_returns_null() {
        assert!(unsafe { mbus_discrete_inputs_values_ptr(core::ptr::null()) }.is_null());
    }

    // ── Correct values ────────────────────────────────────────────────────────

    #[test]
    fn from_address_and_quantity_round_trip() {
        let d = make_discrete(200, 8, 0x00);
        assert_eq!(unsafe { mbus_discrete_inputs_from_address(&d) }, 200);
        assert_eq!(unsafe { mbus_discrete_inputs_quantity(&d) }, 8);
    }

    #[test]
    fn value_reads_correct_bit_by_address() {
        // 0b0000_0011 → bits 0 and 1 set (addresses 20 and 21 ON)
        let d = make_discrete(20, 8, 0b0000_0011);
        let mut v = false;
        assert_eq!(
            unsafe { mbus_discrete_inputs_value(&d, 20, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "addr 20 should be ON");
        assert_eq!(
            unsafe { mbus_discrete_inputs_value(&d, 21, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v, "addr 21 should be ON");
        assert_eq!(
            unsafe { mbus_discrete_inputs_value(&d, 22, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(!v, "addr 22 should be OFF");
    }

    #[test]
    fn value_at_index_is_address_independent() {
        let d = make_discrete(300, 8, 0b0000_0001); // only bit 0 (index 0) set
        let mut v = false;
        assert_eq!(
            unsafe { mbus_discrete_inputs_value_at_index(&d, 0, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(v);
        assert_eq!(
            unsafe { mbus_discrete_inputs_value_at_index(&d, 1, &mut v) },
            MbusStatusCode::MbusOk
        );
        assert!(!v);
    }

    #[test]
    fn value_out_of_range_returns_invalid_address() {
        let d = make_discrete(10, 4, 0x00); // valid addrs 10..13
        let mut v = false;
        assert_eq!(
            unsafe { mbus_discrete_inputs_value(&d, 14, &mut v) },
            MbusStatusCode::MbusErrInvalidAddress
        );
    }

    #[test]
    fn value_at_index_out_of_range_returns_invalid_address() {
        let d = make_discrete(10, 4, 0x00); // valid indices 0..3
        let mut v = false;
        assert_eq!(
            unsafe { mbus_discrete_inputs_value_at_index(&d, 4, &mut v) },
            MbusStatusCode::MbusErrInvalidAddress
        );
    }
}
