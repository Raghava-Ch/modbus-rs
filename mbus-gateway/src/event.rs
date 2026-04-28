//! Gateway observability callbacks.
//!
//! Implement [`GatewayEventHandler`] to receive notifications about gateway
//! activity.  All methods have default no-op bodies so you only need to
//! override the events you care about.

use mbus_core::transport::UnitIdOrSlaveAddr;

/// Observer interface for [`GatewayServices`](crate::services::GatewayServices) events.
///
/// Implement this trait to receive lifecycle and diagnostic notifications from
/// the gateway.  All methods default to no-ops; override only what you need.
pub trait GatewayEventHandler {
    /// A request from `session_id` has been routed to `channel_idx` for `unit`.
    fn on_forward(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr, channel_idx: usize) {
        let _ = (session_id, unit, channel_idx);
    }

    /// A response has been returned to the upstream client for `upstream_txn`.
    fn on_response_returned(&mut self, session_id: u8, upstream_txn: u16) {
        let _ = (session_id, upstream_txn);
    }

    /// No downstream channel was found for `unit`.
    fn on_routing_miss(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr) {
        let _ = (session_id, unit);
    }

    /// The downstream device did not respond within the configured timeout.
    fn on_downstream_timeout(&mut self, session_id: u8, internal_txn: u16) {
        let _ = (session_id, internal_txn);
    }

    /// The upstream session identified by `session_id` has disconnected.
    fn on_upstream_disconnect(&mut self, session_id: u8) {
        let _ = session_id;
    }

    /// Raw bytes received from upstream (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    fn on_upstream_rx(&mut self, session_id: u8, frame: &[u8]) {
        let _ = (session_id, frame);
    }

    /// Raw bytes sent to a downstream channel (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    fn on_downstream_tx(&mut self, channel_idx: usize, frame: &[u8]) {
        let _ = (channel_idx, frame);
    }
}

/// A no-op [`GatewayEventHandler`] that silently discards all events.
///
/// Use this when you don't need observability:
///
/// ```rust
/// use mbus_gateway::NoopEventHandler;
/// let _handler = NoopEventHandler;
/// ```
pub struct NoopEventHandler;

impl GatewayEventHandler for NoopEventHandler {}
