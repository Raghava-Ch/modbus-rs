use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::Transport;

/// An upstream transport plus its receive-accumulation buffer and session ID.
pub struct UpstreamChannel<T: Transport> {
    pub(crate) transport: T,
    pub(crate) rxbuf: Vec<u8, MAX_ADU_FRAME_LEN>,
    /// Monotonic session index (set at registration; used to match responses back).
    pub(crate) session_id: u8,
}

impl<T: Transport> UpstreamChannel<T> {
    pub fn new(transport: T, session_id: u8) -> Self {
        Self {
            transport,
            rxbuf: Vec::new(),
            session_id,
        }
    }

    /// Return an immutable reference to the underlying transport.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Return a mutable reference to the underlying transport.
    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }
}
