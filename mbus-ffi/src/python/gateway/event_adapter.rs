use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::GatewayEventHandler;
use pyo3::prelude::*;

use super::event_handler::GatewayEventHandler as PyGatewayEventHandler;

use std::sync::Arc;

/// Adapts the Rust `GatewayEventHandler` trait to invoke methods on a Python
/// object (`GatewayEventHandler` subclass) using PyO3.
pub struct PyEventAdapter {
    py_handler: Option<Arc<Py<PyGatewayEventHandler>>>,
}

impl PyEventAdapter {
    pub fn new(py_handler: Option<Arc<Py<PyGatewayEventHandler>>>) -> Self {
        Self { py_handler }
    }
}

impl GatewayEventHandler for PyEventAdapter {
    fn on_forward(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr, channel_idx: usize) {
        if let Some(h) = &self.py_handler {
            Python::attach(|py| {
                let unit_id = u8::from(unit);
                if let Err(e) = h.call_method1(py, "on_forward", (session_id, unit_id, channel_idx))
                {
                    e.print(py);
                }
            });
        }
    }

    fn on_response_returned(&mut self, session_id: u8, upstream_txn: u16) {
        if let Some(h) = &self.py_handler {
            Python::attach(|py| {
                if let Err(e) =
                    h.call_method1(py, "on_response_returned", (session_id, upstream_txn))
                {
                    e.print(py);
                }
            });
        }
    }

    fn on_routing_miss(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr) {
        if let Some(h) = &self.py_handler {
            Python::attach(|py| {
                let unit_id = u8::from(unit);
                if let Err(e) = h.call_method1(py, "on_routing_miss", (session_id, unit_id)) {
                    e.print(py);
                }
            });
        }
    }

    fn on_downstream_timeout(&mut self, session_id: u8, internal_txn: u16) {
        if let Some(h) = &self.py_handler {
            Python::attach(|py| {
                if let Err(e) =
                    h.call_method1(py, "on_downstream_timeout", (session_id, internal_txn))
                {
                    e.print(py);
                }
            });
        }
    }

    fn on_upstream_disconnect(&mut self, session_id: u8) {
        if let Some(h) = &self.py_handler {
            Python::attach(|py| {
                if let Err(e) = h.call_method1(py, "on_upstream_disconnect", (session_id,)) {
                    e.print(py);
                }
            });
        }
    }
}
