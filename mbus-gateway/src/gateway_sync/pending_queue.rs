use heapless::Vec;
use mbus_core::data_unit::common::Pdu;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};

/// A single buffered request waiting for a free downstream channel.
#[derive(Clone, Debug)]
pub(crate) struct PendingRequest {
    pub session_idx: usize,
    pub upstream_txn: u16,
    pub unit: UnitIdOrSlaveAddr,
    pub downstream_unit: UnitIdOrSlaveAddr,
    pub fc: FunctionCode,
    pub pdu: Pdu,
    pub upstream_type: TransportType,
}

/// Compile-time bounded pending-request queue.
///
/// `N = 0` — zero-size type; pushing always fails (caller fires `on_downstream_busy`).
/// `N > 0` — buffers up to N requests across all upstream channels.
pub(crate) struct PendingQueue<const N: usize> {
    queue: Vec<PendingRequest, N>,
}

impl<const N: usize> PendingQueue<N> {
    /// Create a new empty pending queue.
    pub const fn new() -> Self {
        Self { queue: Vec::new() }
    }

    /// Push a request to the back of the queue. Returns `true` if successful.
    pub fn push(&mut self, req: PendingRequest) -> bool {
        self.queue.push(req).is_ok()
    }

    /// Remove and return the oldest request (FIFO).
    pub fn pop_front(&mut self) -> Option<PendingRequest> {
        let len = self.queue.len();
        if len == 0 {
            return None;
        }
        for i in 0..len - 1 {
            self.queue.swap(i, i + 1);
        }
        self.queue.pop()
    }

    /// Returns whether the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }
}
