//! Status codes returned by every `mbus_dn_*` entry point.
//!
//! This is a thin .NET-facing wrapper around the existing
//! [`crate::c::error::MbusStatusCode`] enum.  The numeric values are
//! identical so a single C# `enum ModbusStatus` can be used for both the
//! C and .NET headers without re-mapping; we just expose a separate
//! header for cbindgen consumers and keep the namespace tidy.

use core::ffi::c_char;

use mbus_client_async::AsyncError;
use mbus_core::errors::MbusError;

use crate::c::error::MbusStatusCode;

/// Status code returned by every `mbus_dn_*` function.
///
/// Numerically identical to [`crate::c::error::MbusStatusCode`].
pub type MbusDnStatus = MbusStatusCode;

/// Returns a static C string describing `status`.
///
/// Equivalent to [`crate::c::error::mbus_status_str`] but exported with a
/// `mbus_dn_` prefix so that it appears in the .NET-only header.
///
/// The returned pointer is always valid (points to a static string literal).
/// The caller must NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_dn_status_str(status: MbusDnStatus) -> *const c_char {
    crate::c::error::mbus_status_str(status)
}

/// Convert an [`AsyncError`] into a [`MbusDnStatus`].
pub(crate) fn from_async(err: AsyncError) -> MbusDnStatus {
    match err {
        AsyncError::Mbus(e) => MbusStatusCode::from(e),
        AsyncError::WorkerClosed => MbusStatusCode::MbusErrConnectionClosed,
        AsyncError::UnexpectedResponseType => MbusStatusCode::MbusErrUnexpectedResponse,
        AsyncError::Timeout => MbusStatusCode::MbusErrTimeout,
    }
}

/// Convert a synchronously-known [`MbusError`] into a [`MbusDnStatus`].
#[allow(dead_code)]
pub(crate) fn from_mbus(err: MbusError) -> MbusDnStatus {
    MbusStatusCode::from(err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::CStr;

    #[test]
    fn dn_status_str_returns_same_text_as_c_status_str() {
        let dn = unsafe { CStr::from_ptr(mbus_dn_status_str(MbusStatusCode::MbusOk)) };
        let c = unsafe { CStr::from_ptr(crate::c::error::mbus_status_str(MbusStatusCode::MbusOk)) };
        assert_eq!(dn.to_bytes(), c.to_bytes());
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
