//! Gateway observability callbacks.
//!
//! Implement [`GatewayEventHandler`] to receive notifications about gateway
//! activity.  All methods have default no-op bodies so you only need to
//! override the events you care about.

use mbus_core::transport::UnitIdOrSlaveAddr;

/// Observer interface for [`GatewayServices`](crate::GatewayServices) events.
///
/// Implement this trait to receive lifecycle and diagnostic notifications from
/// the gateway.  All methods default to no-ops; override only what you need.
pub trait GatewayEventHandler {
    /// A request from `session_id` has been routed to `channel_idx` for `unit`.
    #[allow(unused_variables)]
    fn on_forward(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr, channel_idx: usize) {}

    /// A response has been returned to the upstream client for `upstream_txn`.
    #[allow(unused_variables)]
    fn on_response_returned(&mut self, session_id: u8, upstream_txn: u16) {}

    /// No downstream channel was found for `unit`.
    #[allow(unused_variables)]
    fn on_routing_miss(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr) {}

    /// The downstream device did not respond within the configured timeout.
    #[allow(unused_variables)]
    fn on_downstream_timeout(&mut self, session_id: u8, internal_txn: u16) {}

    /// The upstream session identified by `session_id` has disconnected.
    #[allow(unused_variables)]
    fn on_upstream_disconnect(&mut self, session_id: u8) {}

    /// All downstream channels are busy; the request for `unit` from `session_id`
    /// could not be forwarded this poll cycle.
    ///
    /// If `N_PENDING > 0`, the request was queued. If `N_PENDING = 0`, it was dropped.
    #[allow(unused_variables)]
    fn on_downstream_busy(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr, queued: bool) {}

    /// Raw bytes received from upstream (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    #[allow(unused_variables)]
    fn on_upstream_rx(&mut self, session_id: u8, frame: &[u8]) {}

    /// Raw bytes sent to a downstream channel (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    #[allow(unused_variables)]
    fn on_downstream_tx(&mut self, channel_idx: usize, frame: &[u8]) {}

    /// Raw bytes received from a downstream channel (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    #[allow(unused_variables)]
    fn on_downstream_rx(&mut self, session_id: u8, channel_idx: usize, frame: &[u8]) {}

    /// Raw bytes sent to the upstream client (requires `traffic` feature).
    #[cfg(feature = "traffic")]
    #[allow(unused_variables)]
    fn on_upstream_tx(&mut self, session_id: u8, frame: &[u8]) {}
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
