pub mod client;
pub mod errors;
pub mod server;

use mbus_core::transport::SerialMode as TransportSerialMode;
use pyo3::prelude::*;

#[pyclass(module = "modbus_rs._modbus_rs", name = "SerialMode", eq, eq_int, skip_from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PySerialMode {
    Rtu = 0,
    Ascii = 1,
}

impl PySerialMode {
    fn into_transport(self) -> TransportSerialMode {
        match self {
            Self::Rtu => TransportSerialMode::Rtu,
            Self::Ascii => TransportSerialMode::Ascii,
        }
    }
}

#[pymethods]
impl PySerialMode {
    #[classattr]
    const RTU: Self = Self::Rtu;

    #[classattr]
    const ASCII: Self = Self::Ascii;
}

pub fn parse_serial_mode_any(mode: &Bound<'_, PyAny>) -> PyResult<TransportSerialMode> {
    if let Ok(mode_enum) = mode.extract::<PyRef<'_, PySerialMode>>() {
        return Ok(mode_enum.into_transport());
    }

    if let Ok(mode_str) = mode.extract::<&str>() {
        return match mode_str.to_lowercase().as_str() {
            "rtu" => Ok(TransportSerialMode::Rtu),
            "ascii" => Ok(TransportSerialMode::Ascii),
            other => Err(errors::ModbusConfigError::new_err(format!(
                "Unknown serial mode '{other}'; expected 'rtu'/'ascii' or SerialMode.RTU/SerialMode.ASCII"
            ))),
        };
    }

    Err(errors::ModbusConfigError::new_err(
        "Invalid serial mode; expected 'rtu'/'ascii' or SerialMode.RTU/SerialMode.ASCII",
    ))
}

/// Entry point registered by Maturin as `modbus_rs._modbus_rs`.
#[pymodule]
pub fn _modbus_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Exceptions
    errors::register_exceptions(m)?;

    // Enums
    m.add_class::<PySerialMode>()?;

    // Client classes
    m.add_class::<client::tcp::TcpClient>()?;
    m.add_class::<client::tcp::AsyncTcpClient>()?;
    m.add_class::<client::serial::SerialClient>()?;
    m.add_class::<client::serial::AsyncSerialClient>()?;

    // Server classes
    m.add_class::<server::app::ModbusApp>()?;
    m.add_class::<server::tcp::AsyncTcpServer>()?;
    m.add_class::<server::tcp::TcpServer>()?;
    m.add_class::<server::serial::AsyncSerialServer>()?;
    m.add_class::<server::serial::SerialServer>()?;

    Ok(())
}
