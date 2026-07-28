#[cfg(feature = "python-client")]
pub mod client;
pub mod errors;
#[cfg(feature = "python-gateway")]
pub mod gateway;
#[cfg(feature = "python-server")]
pub mod server;

use mbus_core::transport::SerialMode as TransportSerialMode;
use pyo3::prelude::*;

#[pyclass(
    module = "modbus_rs._modbus_rs",
    name = "CoilState",
    eq,
    eq_int,
    from_py_object
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PyCoilState {
    Off = 0,
    On = 1,
}

impl PyCoilState {
    pub fn into_core(self) -> mbus_core::models::coil::CoilState {
        match self {
            Self::Off => mbus_core::models::coil::CoilState::Off,
            Self::On => mbus_core::models::coil::CoilState::On,
        }
    }

    pub fn from_core(state: mbus_core::models::coil::CoilState) -> Self {
        match state {
            mbus_core::models::coil::CoilState::Off => Self::Off,
            mbus_core::models::coil::CoilState::On => Self::On,
        }
    }
}

#[pymethods]
impl PyCoilState {
    #[classattr]
    const OFF: Self = Self::Off;

    #[classattr]
    const ON: Self = Self::On;

    fn __repr__(&self) -> &'static str {
        match self {
            Self::On => "CoilState.On",
            Self::Off => "CoilState.Off",
        }
    }

    fn __bool__(&self) -> bool {
        *self == Self::On
    }
}

#[pyclass(
    module = "modbus_rs._modbus_rs",
    name = "SerialMode",
    eq,
    eq_int,
    skip_from_py_object
)]
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
    m.add_class::<PyCoilState>()?;
    m.add_class::<PySerialMode>()?;

    // Client classes
    #[cfg(feature = "python-client")]
    {
        m.add_class::<client::tcp::AsyncTcpTransport>()?;
        m.add_class::<client::tcp::AsyncTcpModbusClient>()?;
        m.add_class::<client::tcp::TcpTransport>()?;
        m.add_class::<client::tcp::TcpModbusClient>()?;
        m.add_class::<client::serial::AsyncRtuTransport>()?;
        m.add_class::<client::serial::AsyncAsciiTransport>()?;
        m.add_class::<client::serial::AsyncSerialModbusClient>()?;
        m.add_class::<client::serial::RtuTransport>()?;
        m.add_class::<client::serial::AsciiTransport>()?;
        m.add_class::<client::serial::SerialModbusClient>()?;
    }

    // Server classes
    #[cfg(feature = "python-server")]
    {
        m.add_class::<server::app::ModbusApp>()?;
        m.add_class::<server::tcp::AsyncTcpServer>()?;
        m.add_class::<server::tcp::TcpServer>()?;
        m.add_class::<server::serial::AsyncSerialServer>()?;
        m.add_class::<server::serial::SerialServer>()?;
    }

    // Gateway classes (feature = "python-gateway")
    #[cfg(feature = "python-gateway")]
    {
        m.add_class::<gateway::event_handler::GatewayEventHandler>()?;
        m.add_class::<gateway::async_tcp::AsyncTcpGateway>()?;
        m.add_class::<gateway::sync_tcp::TcpGateway>()?;
    }

    Ok(())
}
