use mbus_server_async::AsyncTcpServer as InnerAsyncTcpServer;
use mbus_core::transport::UnitIdOrSlaveAddr;
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use std::sync::Arc;
use tokio::sync::Notify;

use super::app::{ModbusApp, PythonAppAdapter};
use crate::python::client::helpers::get_runtime;
use crate::python::errors::async_server_error_to_py;

// ═══════════════════════════════════════════════════════════════════════════
// Async TCP server
// ═══════════════════════════════════════════════════════════════════════════

/// Asyncio Modbus TCP server.
///
/// Pass a :class:`ModbusApp` subclass instance; the server dispatches
/// each incoming request to the corresponding ``handle_*`` method.
///
/// Usage::
///
///     class MyApp(modbus_rs.ModbusApp):
///         def handle_read_holding_registers(self, address, count):
///             return [0] * count
///
///     async def main():
///         async with AsyncTcpServer("0.0.0.0", port=502, app=MyApp(), unit_id=1) as server:
///             await server.serve_forever()
#[pyclass(name = "AsyncTcpServer")]
pub struct AsyncTcpServer {
    bind_addr: String,
    unit_id: u8,
    app: Py<ModbusApp>,
    stop_signal: Arc<Notify>,
}

#[pymethods]
impl AsyncTcpServer {
    /// Create an async TCP server.
    ///
    /// :param host: Bind address (e.g. ``"0.0.0.0"`` or ``"127.0.0.1"``).
    /// :param port: TCP port (default 502).
    /// :param app: A :class:`ModbusApp` subclass instance to handle requests.
    /// :param unit_id: Modbus unit ID to respond to (default 1).
    #[new]
    #[pyo3(signature = (host, app, port=502, unit_id=1))]
    fn new(host: &str, app: Py<ModbusApp>, port: u16, unit_id: u8) -> PyResult<Self> {
        let _ = UnitIdOrSlaveAddr::new(unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let bind_addr = format!("{}:{}", host, port);
        Ok(Self {
            bind_addr,
            unit_id,
            app,
            stop_signal: Arc::new(Notify::new()),
        })
    }

    /// Return the configured TCP bind address as ``"host:port"``.
    fn bind_address(&self) -> String {
        self.bind_addr.clone()
    }

    /// Bind and serve until :meth:`stop` is called or an error occurs.
    ///
    /// Returns normally (``None``) when :meth:`stop` has been called.
    /// Raises on a fatal bind or transport error.
    fn serve_forever<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let addr = self.bind_addr.clone();
        let unit = UnitIdOrSlaveAddr::new(self.unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let adapter = PythonAppAdapter::new(self.app.clone_ref(py));
        let stop_signal = self.stop_signal.clone();
        future_into_py(py, async move {
            InnerAsyncTcpServer::serve_with_shutdown(
                addr.as_str(),
                adapter,
                unit,
                stop_signal.notified(),
            )
            .await
            .map_err(async_server_error_to_py)
        })
    }

    /// Signal the server to stop accepting new connections.
    ///
    /// After this call, :meth:`serve_forever` returns.  In-flight sessions
    /// finish normally.  The server can be started again by calling
    /// :meth:`serve_forever` once more.
    fn stop<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let stop_signal = self.stop_signal.clone();
        future_into_py(py, async move {
            stop_signal.notify_one();
            Ok::<(), PyErr>(())
        })
    }

    /// Enter the async context manager.
    ///
    /// This method returns ``self`` and does not implicitly start serving.
    /// Use :meth:`serve_forever` to run the listener.
    fn __aenter__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let _ = UnitIdOrSlaveAddr::new(slf.unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
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
// Sync TCP server
// ═══════════════════════════════════════════════════════════════════════════

/// Synchronous (blocking) Modbus TCP server.
///
/// Blocks the calling thread until an error or shutdown.
/// Use :class:`AsyncTcpServer` if you need non-blocking I/O.
///
/// Usage::
///
///     server = TcpServer("0.0.0.0", port=502, app=MyApp(), unit_id=1)
///     server.serve_forever()  # blocks
#[pyclass(name = "TcpServer")]
pub struct TcpServer {
    bind_addr: String,
    unit_id: u8,
    app: Py<ModbusApp>,
    stop_signal: Arc<Notify>,
}

#[pymethods]
impl TcpServer {
    #[new]
    #[pyo3(signature = (host, app, port=502, unit_id=1))]
    fn new(host: &str, app: Py<ModbusApp>, port: u16, unit_id: u8) -> PyResult<Self> {
        let _ = UnitIdOrSlaveAddr::new(unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let bind_addr = format!("{}:{}", host, port);
        Ok(Self {
            bind_addr,
            unit_id,
            app,
            stop_signal: Arc::new(Notify::new()),
        })
    }

    /// Return the configured TCP bind address as ``"host:port"``.
    fn bind_address(&self) -> String {
        self.bind_addr.clone()
    }

    /// Block and serve until :meth:`stop` is called. Releases the GIL during I/O.
    fn serve_forever(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        let addr = self.bind_addr.clone();
        let unit = UnitIdOrSlaveAddr::new(self.unit_id)
            .map_err(crate::python::errors::mbus_error_to_py)?;
        let adapter = PythonAppAdapter::new(self.app.clone_ref(py));
        let stop_signal = self.stop_signal.clone();
        py.detach(|| {
            rt.block_on(
                InnerAsyncTcpServer::serve_with_shutdown(
                    addr.as_str(),
                    adapter,
                    unit,
                    stop_signal.notified(),
                ),
            )
            .map(|_| ())
            .map_err(async_server_error_to_py)
        })
    }

    /// Signal the blocking server loop to stop.
    fn stop(&self) {
        self.stop_signal.notify_one();
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
        self.stop_signal.notify_one();
        false
    }
}
