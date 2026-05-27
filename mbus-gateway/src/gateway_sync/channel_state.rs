use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};

/// Per-downstream-channel state for the non-blocking sync gateway.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ChannelState {
    /// No pending transaction; this channel is free to accept a new request.
    Idle,

    /// A request has been forwarded downstream; we are waiting for the response.
    AwaitingResponse {
        /// Gateway-assigned internal transaction ID.
        internal_txn: u16,
        /// Index into `GatewayServices::upstreams` that originated this request.
        session_idx: usize,
        /// Absolute `now_ms` value at which this transaction expires.
        deadline_ms: u64,
        /// Original upstream transaction ID (restored in the response).
        upstream_txn: u16,
        /// Upstream unit ID (used for response re-encoding).
        unit: UnitIdOrSlaveAddr,
        /// Function code (used for exception generation on timeout).
        fc: FunctionCode,
        /// The transport type of the upstream that originated the request.
        upstream_type: TransportType,
    },
}

impl ChannelState {
    /// Returns `true` when the channel is free.
    #[inline]
    pub(crate) fn is_idle(&self) -> bool {
        matches!(self, ChannelState::Idle)
    }
}
