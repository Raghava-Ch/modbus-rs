use std::str::FromStr;
use std::sync::Arc;

use mbus_server_async::{AsyncRtuServer, AsyncAsciiServer};
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, ModbusConfig, Parity, SerialMode, UnitIdOrSlaveAddr,
};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::Notify;

use super::app::{ModbusApp, PythonAppAdapter};
use crate::python::client::helpers::get_runtime;
use crate::python::errors::async_server_error_to_py;

// ── shared helpers ────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn make_serial_config(
    port: &str,
    baud_rate: u32,
    timeout_ms: u64,
    mode: SerialMode,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: u8,
    retry_attempts: u8,
) -> PyResult<ModbusConfig> {
    let port_path = heapless::String::<64>::from_str(port)
        .map_err(|_| crate::python::errors::ModbusConfigError::new_err(
            format!("Port path too long (max 64 chars): {port}"),
        ))?;
    let serial_cfg = ModbusSerialConfig {
        port_path,
        mode,
        baud_rate: normalize_baud_rate(baud_rate),
        data_bits,
        stop_bits,
        parity,
        response_timeout_ms: timeout_ms as u32,
        retry_attempts,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    Ok(ModbusConfig::Serial(serial_cfg))
}

fn normalize_baud_rate(baud_rate: u32) -> BaudRate {
    match baud_rate {
        9600 => BaudRate::Baud9600,
        19200 => BaudRate::Baud19200,
        other => BaudRate::Custom(other),
    }
}

fn parse_data_bits(data_bits: u8) -> PyResult<DataBits> {
    match data_bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Invalid data_bits '{other}'; expected one of 5, 6, 7, or 8"
        ))),
    }
}

fn parse_parity(parity: &str) -> PyResult<Parity> {
    match parity.to_lowercase().as_str() {
        "none" | "n" => Ok(Parity::None),
        "even" | "e" => Ok(Parity::Even),
        "odd" | "o" => Ok(Parity::Odd),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Unknown parity '{other}'; expected 'none', 'even', or 'odd'"
        ))),
    }
}

fn parse_stop_bits(stop_bits: u8) -> PyResult<u8> {
    match stop_bits {
        1 | 2 => Ok(stop_bits),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Invalid stop_bits '{other}'; expected 1 or 2"
        ))),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Async Serial server
// ═══════════════════════════════════════════════════════════════════════════

/// Asyncio Modbus serial server (RTU or ASCII).
///
/// Runs a single-connection serial Modbus server on the given port.
///
/// Usage::
///
/// ```python
/// async with AsyncSerialServer("/dev/ttyUSB0", app=MyApp(), unit_id=1) as srv:
///     await srv.serve_forever()
/// ```
#[pyclass(name = "AsyncSerialServer")]
pub struct AsyncSerialServer {
    port: String,
    baud_rate: u32,
    unit_id: u8,
    mode: SerialMode,
    timeout_ms: u64,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: u8,
    retry_attempts: u8,
    app: Py<ModbusApp>,
    stop_signal: Arc<Notify>,
}

#[pymethods]
impl AsyncSerialServer {
    #[new]
    #[pyo3(signature = (port, app, baud_rate=9600, unit_id=1, mode=None, timeout_ms=1000, data_bits=8, parity="none", stop_bits=1, retry_attempts=0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        port: &str,
        app: Py<ModbusApp>,
        baud_rate: u32,
        unit_id: u8,
        mode: Option<Py<PyAny>>,
        timeout_ms: u64,
        data_bits: u8,
        parity: &str,
        stop_bits: u8,
        retry_attempts: u8,
    ) -> PyResult<Self> {
        let serial_mode = match mode {
            Some(mode) => crate::python::parse_serial_mode_any(mode.bind(py))?,
            None => SerialMode::Rtu,
        };
        let data_bits = parse_data_bits(data_bits)?;
        let parity = parse_parity(parity)?;
        let stop_bits = parse_stop_bits(stop_bits)?;
        let _ = UnitIdOrSlaveAddr::new(unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        Ok(Self {
            port: port.to_string(),
            baud_rate,
            unit_id,
            mode: serial_mode,
            timeout_ms,
            data_bits,
            parity,
            stop_bits,
            retry_attempts,
            app,
            stop_signal: Arc::new(Notify::new()),
        })
    }

    /// Run the server loop until :meth:`stop` is called or the port closes.
    ///
    /// Returns normally when :meth:`stop` has been called.  Raises on a
    /// fatal transport error.
    fn serve_forever<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let config = make_serial_config(
            &self.port,
            self.baud_rate,
            self.timeout_ms,
            self.mode,
            self.data_bits,
            self.parity,
            self.stop_bits,
            self.retry_attempts,
        )?;
        let unit = UnitIdOrSlaveAddr::new(self.unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let adapter = PythonAppAdapter::new(self.app.clone_ref(py));
        let mode = self.mode;
        let stop_signal = self.stop_signal.clone();
        future_into_py(py, async move {
            match mode {
                SerialMode::Rtu => {
                    let mut srv = AsyncRtuServer::new_rtu(&config, unit)
                        .map_err(async_server_error_to_py)?;
                    srv.run_with_shutdown(adapter, stop_signal.notified())
                        .await
                        .map_err(async_server_error_to_py)
                }
                SerialMode::Ascii => {
                    let mut srv = AsyncAsciiServer::new_ascii(&config, unit)
                        .map_err(async_server_error_to_py)?;
                    srv.run_with_shutdown(adapter, stop_signal.notified())
                        .await
                        .map_err(async_server_error_to_py)
                }
            }
        })
    }

    /// Signal the server to stop.
    ///
    /// After this call, :meth:`serve_forever` returns.  The server can be
    /// restarted by calling :meth:`serve_forever` again.
    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stop_signal = self.stop_signal.clone();
        future_into_py(py, async move {
            stop_signal.notify_one();
            Ok::<(), PyErr>(())
        })
    }

    fn __aenter__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let this = slf.into_pyobject(py)?.into_any().unbind();
        future_into_py(py, async move { Ok::<Py<PyAny>, PyErr>(this) })
    }

    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_val: Option<Bound<'py, PyAny>>,
        _exc_tb: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let stop_signal = self.stop_signal.clone();
        future_into_py(py, async move {
            stop_signal.notify_one();
            Ok::<bool, PyErr>(false)
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync Serial server
// ═══════════════════════════════════════════════════════════════════════════

/// Synchronous (blocking) Modbus serial server.
#[pyclass(name = "SerialServer")]
pub struct SerialServer {
    port: String,
    baud_rate: u32,
    unit_id: u8,
    mode: SerialMode,
    timeout_ms: u64,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: u8,
    retry_attempts: u8,
    app: Py<ModbusApp>,
}

#[pymethods]
impl SerialServer {
    #[new]
    #[pyo3(signature = (port, app, baud_rate=9600, unit_id=1, mode=None, timeout_ms=1000, data_bits=8, parity="none", stop_bits=1, retry_attempts=0))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        port: &str,
        app: Py<ModbusApp>,
        baud_rate: u32,
        unit_id: u8,
        mode: Option<Py<PyAny>>,
        timeout_ms: u64,
        data_bits: u8,
        parity: &str,
        stop_bits: u8,
        retry_attempts: u8,
    ) -> PyResult<Self> {
        let serial_mode = match mode {
            Some(mode) => crate::python::parse_serial_mode_any(mode.bind(py))?,
            None => SerialMode::Rtu,
        };
        let data_bits = parse_data_bits(data_bits)?;
        let parity = parse_parity(parity)?;
        let stop_bits = parse_stop_bits(stop_bits)?;
        let _ = UnitIdOrSlaveAddr::new(unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        Ok(Self {
            port: port.to_string(),
            baud_rate,
            unit_id,
            mode: serial_mode,
            timeout_ms,
            data_bits,
            parity,
            stop_bits,
            retry_attempts,
            app,
        })
    }

    fn serve_forever(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        let config = make_serial_config(
            &self.port,
            self.baud_rate,
            self.timeout_ms,
            self.mode,
            self.data_bits,
            self.parity,
            self.stop_bits,
            self.retry_attempts,
        )?;
        let unit = UnitIdOrSlaveAddr::new(self.unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let adapter = PythonAppAdapter::new(self.app.clone_ref(py));
        let mode = self.mode;
        py.detach(|| {
            rt.block_on(async move {
                match mode {
                    SerialMode::Rtu => {
                        let mut srv = AsyncRtuServer::new_rtu(&config, unit)
                            .map_err(async_server_error_to_py)?;
                        srv.run(adapter).await.map_err(async_server_error_to_py)
                    }
                    SerialMode::Ascii => {
                        let mut srv = AsyncAsciiServer::new_ascii(&config, unit)
                            .map_err(async_server_error_to_py)?;
                        srv.run(adapter).await.map_err(async_server_error_to_py)
                    }
                }
            })
        })
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(
        &self,
        _py: Python<'_>,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_val: Option<Bound<'_, PyAny>>,
        _exc_tb: Option<Bound<'_, PyAny>>,
    ) -> bool {
        false
    }
}
