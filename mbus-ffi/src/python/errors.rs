use mbus_client_async::AsyncError;
use mbus_server_async::AsyncServerError;
use mbus_core::errors::MbusError;
use pyo3::prelude::*;
use pyo3::{create_exception, exceptions::PyException};

// ── Exception hierarchy ──────────────────────────────────────────────────────

create_exception!(modbus_rs, ModbusError, PyException,
    "Base class for all modbus-rs exceptions.");

create_exception!(modbus_rs, ModbusTimeout, ModbusError,
    "Raised when a Modbus request times out before a response is received.");

create_exception!(modbus_rs, ModbusConnectionError, ModbusError,
    "Raised when the connection to the Modbus device fails or is lost.");

create_exception!(modbus_rs, ModbusProtocolError, ModbusError,
    "Raised when a Modbus protocol framing or parse error occurs.");

create_exception!(modbus_rs, ModbusDeviceException, ModbusProtocolError,
    "Raised when the remote device returns a Modbus exception response (exception code in args[1]).");

create_exception!(modbus_rs, ModbusConfigError, ModbusError,
    "Raised when client or server configuration is invalid (e.g. bad port, bad unit ID at construction).");

create_exception!(modbus_rs, ModbusInvalidArgument, ModbusError,
    "Raised when a per-request argument is out of range or otherwise invalid \
    (e.g. address out of range, invalid quantity, bad coil value, invalid mask).");

// ── Conversion helpers ───────────────────────────────────────────────────────

/// Convert an `AsyncError` into a `PyErr`, mapping onto the exception hierarchy.
pub fn async_error_to_py(err: AsyncError) -> PyErr {
    match err {
        AsyncError::Timeout => ModbusTimeout::new_err("Request timed out"),
        AsyncError::WorkerClosed => {
            ModbusConnectionError::new_err("Background worker closed — reconnect before retrying")
        }
        AsyncError::UnexpectedResponseType => {
            ModbusProtocolError::new_err("Unexpected response type from server")
        }
        AsyncError::Mbus(mbus) => mbus_error_to_py(mbus),
    }
}

/// Convert an `MbusError` into a `PyErr`.
pub fn mbus_error_to_py(err: MbusError) -> PyErr {
    match err {
        MbusError::Timeout => ModbusTimeout::new_err("Request timed out"),
        MbusError::ModbusException(code) => {
            ModbusDeviceException::new_err((format!("Device exception code 0x{code:02X}"), code))
        }
        MbusError::ConnectionLost
        | MbusError::ConnectionFailed
        | MbusError::ConnectionClosed
        | MbusError::IoError => {
            ModbusConnectionError::new_err(format!("{err}"))
        }
        MbusError::ParseError
        | MbusError::BasicParseError
        | MbusError::InvalidPduLength
        | MbusError::InvalidAduLength
        | MbusError::ChecksumError
        | MbusError::UnexpectedResponse => {
            ModbusProtocolError::new_err(format!("{err}"))
        }
        // Setup/construction errors — bad configuration supplied at build time.
        MbusError::InvalidConfiguration
        | MbusError::InvalidSlaveAddress
        | MbusError::InvalidBroadcastAddress => ModbusConfigError::new_err(format!("{err}")),
        // Per-call argument errors — bad value supplied to a request method.
        MbusError::InvalidAddress
        | MbusError::InvalidQuantity
        | MbusError::InvalidValue
        | MbusError::InvalidAndMask
        | MbusError::InvalidOrMask
        | MbusError::InvalidByteCount
        | MbusError::InvalidDataLen
        | MbusError::InvalidOffset => ModbusInvalidArgument::new_err(format!("{err}")),
        _ => ModbusError::new_err(format!("{err}")),
    }
}

/// Convert an `AsyncServerError` into a `PyErr`.
pub fn async_server_error_to_py(err: AsyncServerError) -> PyErr {
    match err {
        AsyncServerError::ConnectionClosed => {
            ModbusConnectionError::new_err("Client connection closed")
        }
        AsyncServerError::Transport(e) => mbus_error_to_py(e),
        AsyncServerError::FramingError(e) => {
            ModbusProtocolError::new_err(format!("Framing error: {e}"))
        }
        AsyncServerError::BindFailed(e) => {
            ModbusConfigError::new_err(format!("Server bind failed: {e}"))
        }
    }
}

/// Register all exception classes onto the given module.
pub fn register_exceptions(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("ModbusError", m.py().get_type::<ModbusError>())?;
    m.add("ModbusTimeout", m.py().get_type::<ModbusTimeout>())?;
    m.add(
        "ModbusConnectionError",
        m.py().get_type::<ModbusConnectionError>(),
    )?;
    m.add(
        "ModbusProtocolError",
        m.py().get_type::<ModbusProtocolError>(),
    )?;
    m.add(
        "ModbusDeviceException",
        m.py().get_type::<ModbusDeviceException>(),
    )?;
    m.add("ModbusConfigError", m.py().get_type::<ModbusConfigError>())?;
    m.add(
        "ModbusInvalidArgument",
        m.py().get_type::<ModbusInvalidArgument>(),
    )?;
    Ok(())
}
