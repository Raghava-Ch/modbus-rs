//! Error conversion utilities for napi-rs.
//!
//! Maps Rust Modbus errors to napi::Error with stable error code strings.

use core::fmt::Display;
use mbus_client_async::AsyncError;
use mbus_core::errors::{ExceptionCode, MbusError};
use napi::Status;

/// Error code strings for JS-side error handling.
pub const ERR_MODBUS_EXCEPTION: &str = "MODBUS_EXCEPTION";
pub const ERR_MODBUS_TIMEOUT: &str = "MODBUS_TIMEOUT";
pub const ERR_MODBUS_TRANSPORT: &str = "MODBUS_TRANSPORT";
pub const ERR_MODBUS_INVALID_ARGUMENT: &str = "MODBUS_INVALID_ARGUMENT";
pub const ERR_MODBUS_CONNECTION_CLOSED: &str = "MODBUS_CONNECTION_CLOSED";
pub const ERR_MODBUS_INTERNAL: &str = "MODBUS_INTERNAL";

/// Converts an `AsyncError` to a napi::Error with appropriate status and message.
pub fn from_async_error(e: AsyncError) -> napi::Error {
    match e {
        AsyncError::Timeout => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_TIMEOUT}] Request timed out"))
        }
        AsyncError::WorkerClosed => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_CONNECTION_CLOSED}] Worker task closed"))
        }
        AsyncError::UnexpectedResponseType => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_INTERNAL}] Unexpected response type"))
        }
        AsyncError::Mbus(mbus_err) => from_mbus_error(mbus_err),
    }
}

/// Converts an `MbusError` to a napi::Error.
pub fn from_mbus_error(e: MbusError) -> napi::Error {
    match e {
        MbusError::ModbusException(code) => {
            napi::Error::new(
                Status::GenericFailure,
                format!("[{ERR_MODBUS_EXCEPTION}:{code}] Modbus exception code: {code}"),
            )
        }
        MbusError::Timeout => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_TIMEOUT}] Transport timeout"))
        }
        MbusError::ConnectionClosed => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_CONNECTION_CLOSED}] Connection closed"))
        }
        MbusError::InvalidSlaveAddress => {
            napi::Error::new(
                Status::InvalidArg,
                format!("[{ERR_MODBUS_INVALID_ARGUMENT}] Invalid slave address"),
            )
        }
        MbusError::InvalidAddress => {
            napi::Error::new(
                Status::InvalidArg,
                format!("[{ERR_MODBUS_INVALID_ARGUMENT}] Invalid address"),
            )
        }
        other => {
            napi::Error::new(Status::GenericFailure, format!("[{ERR_MODBUS_TRANSPORT}] {other}"))
        }
    }
}

/// Helper to convert any Display-able error to a napi::Error with a prefix.
pub fn to_napi_err<E: Display>(prefix: &str, e: E) -> napi::Error {
    napi::Error::new(Status::GenericFailure, format!("[{prefix}] {e}"))
}

/// Convert ExceptionCode to its numeric value for error messages.
#[allow(dead_code)]
fn exception_code_to_u8(code: ExceptionCode) -> u8 {
    u8::from(code)
}
