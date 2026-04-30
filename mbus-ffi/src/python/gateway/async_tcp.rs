//! Asyncio Modbus TCP gateway binding.

use std::sync::{Arc, Mutex as StdMutex};

use mbus_gateway::AsyncTcpGatewayServer;
use mbus_network::TokioTcpTransport;
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use tokio::sync::{Mutex as TokioMutex, Notify};

use super::composite_router::PyRouter;
use super::event_handler::GatewayEventHandler;

#[derive(Clone)]
struct GatewayConfig {
    bind_addr: String,
    downstreams: Vec<(String, u16)>,
    router: PyRouter,
}

/// Asyncio Modbus TCP→TCP gateway.
///
/// Accepts upstream Modbus TCP connections on ``bind_addr`` and forwards
/// requests to one of the configured downstream TCP endpoints based on the
/// unit/range routing rules.
///
/// Usage::
///
/// ```python
/// gw = AsyncTcpGateway("127.0.0.1:5020")
/// ch = gw.add_tcp_downstream("192.168.1.10", 502)
/// gw.add_unit_route(unit=1, channel=ch)
///
/// async def main():
///     await gw.serve_forever()
/// ```
#[pyclass(name = "AsyncTcpGateway")]
pub struct AsyncTcpGateway {
    config: Arc<StdMutex<GatewayConfig>>,
    stop_signal: Arc<Notify>,
    #[allow(dead_code)] // forward-compat: async server has no event-handler hook yet
    event_handler: Option<Py<GatewayEventHandler>>,
}

#[pymethods]
impl AsyncTcpGateway {
    /// :param bind_addr: ``"host:port"`` upstream listen address.
    /// :param event_handler: Optional :class:`GatewayEventHandler` instance.
    ///     Currently stored but not invoked — the underlying async server has
    ///     no event-hook surface yet.
    #[new]
    #[pyo3(signature = (bind_addr, event_handler=None))]
    fn new(bind_addr: &str, event_handler: Option<Py<GatewayEventHandler>>) -> Self {
        Self {
            config: Arc::new(StdMutex::new(GatewayConfig {
                bind_addr: bind_addr.to_owned(),
                downstreams: Vec::new(),
                router: PyRouter::new(),
            })),
            stop_signal: Arc::new(Notify::new()),
            event_handler,
        }
    }

    /// Return the configured upstream bind address.
    fn bind_address(&self) -> String {
        self.config.lock().unwrap().bind_addr.clone()
    }

    /// Register a TCP downstream channel and return its zero-based index.
    ///
    /// :param host: Downstream host name or IP.
    /// :param port: Downstream TCP port (default 502).
    fn add_tcp_downstream(&self, host: &str, port: Option<u16>) -> usize {
        let mut cfg = self.config.lock().unwrap();
        cfg.downstreams.push((host.to_owned(), port.unwrap_or(502)));
        cfg.downstreams.len() - 1
    }

    /// Map a single unit ID to a channel.
    fn add_unit_route(&self, unit: u8, channel: usize) -> PyResult<()> {
        if unit == 0 {
            return Err(PyValueError::new_err("unit ID must be 1..=247"));
        }
        let mut cfg = self.config.lock().unwrap();
        if channel >= cfg.downstreams.len() {
            return Err(PyValueError::new_err(format!(
                "channel {channel} not registered (only {} downstream(s) configured)",
                cfg.downstreams.len()
            )));
        }
        cfg.router.add_unit(unit, channel);
        Ok(())
    }

    /// Map an inclusive unit-ID range to a channel.
    fn add_range_route(&self, min: u8, max: u8, channel: usize) -> PyResult<()> {
        if min == 0 || max < min {
            return Err(PyValueError::new_err("invalid range: require 1 <= min <= max"));
        }
        let mut cfg = self.config.lock().unwrap();
        if channel >= cfg.downstreams.len() {
            return Err(PyValueError::new_err(format!(
                "channel {channel} not registered (only {} downstream(s) configured)",
                cfg.downstreams.len()
            )));
        }
        cfg.router.add_range(min, max, channel);
        Ok(())
    }

    /// Bind, accept, and serve until :meth:`stop` is called.
    ///
    /// Returns ``None`` when shutdown completes; raises on bind/accept errors.
    fn serve_forever<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let cfg_snapshot = self.config.lock().unwrap().clone();
        let stop = self.stop_signal.clone();

        if cfg_snapshot.downstreams.is_empty() {
            return Err(PyValueError::new_err(
                "at least one downstream must be registered before serve_forever()",
            ));
        }
        if cfg_snapshot.router.is_empty() {
            return Err(PyValueError::new_err(
                "at least one route must be registered before serve_forever()",
            ));
        }

        future_into_py(py, async move {
            // Connect each downstream once and wrap in Arc<Mutex<>>.
            let mut downstreams = Vec::with_capacity(cfg_snapshot.downstreams.len());
            for (host, port) in &cfg_snapshot.downstreams {
                let t = TokioTcpTransport::connect((host.as_str(), *port))
                    .await
                    .map_err(|e| {
                        PyConnectionError::new_err(format!(
                            "downstream connect to {host}:{port} failed: {e}"
                        ))
                    })?;
                downstreams.push(Arc::new(TokioMutex::new(t)));
            }

            AsyncTcpGatewayServer::serve_with_shutdown(
                cfg_snapshot.bind_addr.as_str(),
                cfg_snapshot.router,
                downstreams,
                stop.notified(),
            )
            .await
            .map_err(|e| PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Signal :meth:`serve_forever` to stop accepting new connections.
    fn stop(&self) {
        self.stop_signal.notify_one();
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
        let stop = self.stop_signal.clone();
        future_into_py(py, async move {
            stop.notify_one();
            Ok::<bool, PyErr>(false)
        })
    }
}
