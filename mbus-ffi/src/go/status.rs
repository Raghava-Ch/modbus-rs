//! Status codes returned by every `mbus_go_*` entry point.
//!
//! Thin Go-facing wrapper around the existing
//! [`crate::c::error::MbusStatusCode`] enum.  The numeric values are
//! identical so the Go binding can map the integer directly onto its own
//! `modbus.Status` enum without re-mapping.  The `mbus_go_*` prefix keeps
//! the symbol namespace tidy and lets the Go header
//! (`modbus_rs_go.h`) be self-contained.

use core::ffi::c_char;

use mbus_client_async::AsyncError;
use mbus_core::errors::MbusError;

use crate::c::error::MbusStatusCode;

/// Status code returned by every `mbus_go_*` function.
///
/// Numerically identical to [`crate::c::error::MbusStatusCode`].
pub type MbusGoStatus = MbusStatusCode;

/// Returns a static C string describing `status`.
///
/// Equivalent to [`crate::c::error::mbus_status_str`] but exported with a
/// `mbus_go_` prefix so that it appears in the Go-only header.
///
/// The returned pointer is always valid (points to a static string literal).
/// The caller must NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_go_status_str(status: MbusGoStatus) -> *const c_char {
    crate::c::error::mbus_status_str(status)
}

/// Convert an [`AsyncError`] into a [`MbusGoStatus`].
pub(crate) fn from_async(err: AsyncError) -> MbusGoStatus {
    match err {
        AsyncError::Mbus(e) => MbusStatusCode::from(e),
        AsyncError::WorkerClosed => MbusStatusCode::MbusErrConnectionClosed,
        AsyncError::UnexpectedResponseType => MbusStatusCode::MbusErrUnexpectedResponse,
        AsyncError::Timeout => MbusStatusCode::MbusErrTimeout,
    }
}

/// Convert a synchronously-known [`MbusError`] into a [`MbusGoStatus`].
#[allow(dead_code)]
pub(crate) fn from_mbus(err: MbusError) -> MbusGoStatus {
    MbusStatusCode::from(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::CStr;

    #[test]
    fn go_status_str_returns_same_text_as_c_status_str() {
        let go = unsafe { CStr::from_ptr(mbus_go_status_str(MbusStatusCode::MbusOk)) };
        let c = unsafe { CStr::from_ptr(crate::c::error::mbus_status_str(MbusStatusCode::MbusOk)) };
        assert_eq!(go.to_bytes(), c.to_bytes());
    }

    #[test]
    fn from_async_maps_every_variant() {
        assert_eq!(
            from_async(AsyncError::Timeout),
            MbusStatusCode::MbusErrTimeout
        );
        assert_eq!(
            from_async(AsyncError::WorkerClosed),
            MbusStatusCode::MbusErrConnectionClosed
        );
        assert_eq!(
            from_async(AsyncError::UnexpectedResponseType),
            MbusStatusCode::MbusErrUnexpectedResponse
        );
        assert_eq!(
            from_async(AsyncError::Mbus(MbusError::ConnectionFailed)),
            MbusStatusCode::MbusErrConnectionFailed
        );
    }
}
