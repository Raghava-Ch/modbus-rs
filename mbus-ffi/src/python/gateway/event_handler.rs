//! Forward-compatible Python `GatewayEventHandler` base class.
//!
//! `mbus_gateway::AsyncTcpGatewayServer::serve_with_shutdown` does not yet
//! accept an event handler, so instances of this class are stored but never
//! invoked from the binding. The class is exported so that user code written
//! today continues to work unchanged when the async gateway server gains
//! event-hook support.

use pyo3::prelude::*;

/// Subclass to receive gateway lifecycle events.
///
/// All methods are no-ops by default. Override only those you need.
#[pyclass(name = "GatewayEventHandler", subclass)]
pub struct GatewayEventHandler;

#[pymethods]
impl GatewayEventHandler {
    #[new]
    fn new() -> Self {
        Self
    }

    /// Called when the gateway forwards a request to a downstream channel.
    #[allow(unused_variables)]
    fn on_forward(&self, session_id: u8, unit_id: u8, channel_idx: u16) {}

    /// Called when a downstream response has been returned upstream.
    #[allow(unused_variables)]
    fn on_response_returned(&self, session_id: u8, upstream_txn: u16) {}

    /// Called when no route matches the unit ID in an upstream request.
    #[allow(unused_variables)]
    fn on_routing_miss(&self, session_id: u8, unit_id: u8) {}

    /// Called when a downstream did not respond before the timeout.
    #[allow(unused_variables)]
    fn on_downstream_timeout(&self, session_id: u8, internal_txn: u16) {}

    /// Called when the upstream session disconnects.
    #[allow(unused_variables)]
    fn on_upstream_disconnect(&self, session_id: u8) {}
}
