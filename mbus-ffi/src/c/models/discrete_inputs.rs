use mbus_core::models::discrete_input::DiscreteInputs;
#[cfg(feature = "discrete-inputs")]
use crate::c::MbusStatusCode;

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
pub unsafe extern "C" fn mbus_discrete_inputs_from_address(discrete_inputs: *const MbusDiscreteInputs) -> u16 {
    if discrete_inputs.is_null() {
        return 0;
    }
    unsafe { (*discrete_inputs).inner().from_address() }
}

#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
/// Returns the number of discrete inputs.
pub unsafe extern "C" fn mbus_discrete_inputs_quantity(discrete_inputs: *const MbusDiscreteInputs) -> u16 {
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
/// Returns a raw pointer to the discrete input bit-values. Valid during callback only.
pub unsafe extern "C" fn mbus_discrete_inputs_values_ptr(discrete_inputs: *const MbusDiscreteInputs) -> *const u8 {
    if discrete_inputs.is_null() {
        return core::ptr::null();
    }
    unsafe { (*discrete_inputs).inner().values().as_ptr() }
}
