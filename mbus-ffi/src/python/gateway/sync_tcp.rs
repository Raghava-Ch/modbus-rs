//! Blocking Modbus TCP gateway binding (driven by the shared Tokio runtime).

use std::sync::{Arc, Mutex as StdMutex};

use mbus_gateway::AsyncTcpGatewayServer;
use mbus_network::TokioTcpTransport;
use pyo3::exceptions::{PyConnectionError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use tokio::sync::{Mutex as TokioMutex, Notify};

use super::composite_router::PyRouter;
use super::event_handler::GatewayEventHandler;
use crate::python::client::helpers::get_runtime;

#[derive(Clone)]
struct GatewayConfig {
    bind_addr: String,
    downstreams: Vec<(String, u16)>,
    router: PyRouter,
}

/// Blocking Modbus TCP→TCP gateway.
///
/// Identical configuration surface as :class:`AsyncTcpGateway`, but
/// :meth:`serve_forever` blocks the calling thread (the GIL is released
/// while the gateway is running). Use this when not running in an asyncio
/// event loop.
#[pyclass(name = "TcpGateway")]
pub struct TcpGateway {
    config: Arc<StdMutex<GatewayConfig>>,
    stop_signal: Arc<Notify>,
    #[allow(dead_code)]
    event_handler: Option<Py<GatewayEventHandler>>,
}

#[pymethods]
impl TcpGateway {
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

    fn bind_address(&self) -> String {
        self.config.lock().unwrap().bind_addr.clone()
    }

    fn add_tcp_downstream(&self, host: &str, port: Option<u16>) -> usize {
        let mut cfg = self.config.lock().unwrap();
        cfg.downstreams.push((host.to_owned(), port.unwrap_or(502)));
        cfg.downstreams.len() - 1
    }

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

    /// Block and serve until :meth:`stop` is called from another thread.
    fn serve_forever(&self, py: Python<'_>) -> PyResult<()> {
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

        let rt = get_runtime();
        py.detach(|| {
            rt.block_on(async move {
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
        })
    }

    fn stop(&self) {
        self.stop_signal.notify_one();
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(
        &self,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_val: Option<Bound<'_, PyAny>>,
        _exc_tb: Option<Bound<'_, PyAny>>,
    ) -> bool {
        self.stop_signal.notify_one();
        false
    }
}
