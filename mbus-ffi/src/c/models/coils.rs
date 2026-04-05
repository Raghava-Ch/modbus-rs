use mbus_core::models::coil::Coils;
#[cfg(feature = "coils")]
use crate::c::MbusStatusCode;

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
pub unsafe extern "C" fn mbus_coils_from_address(coils: *const MbusCoils) -> u16 {
    if coils.is_null() {
        return 0;
    }
    unsafe { (*coils).inner().from_address() }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Returns the number of coils.
pub unsafe extern "C" fn mbus_coils_quantity(coils: *const MbusCoils) -> u16 {
    if coils.is_null() {
        return 0;
    }
    unsafe { (*coils).inner().quantity() }
}

#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
/// Reads a single coil value by address into `out_value`.
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
/// Returns a raw pointer to the packed coil bit-values. Valid during callback only.
pub unsafe extern "C" fn mbus_coils_values_ptr(coils: *const MbusCoils) -> *const u8 {
    if coils.is_null() {
        return core::ptr::null();
    }
    unsafe { (*coils).inner().values().as_ptr() }
}
