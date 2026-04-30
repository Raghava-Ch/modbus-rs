//! Downstream channel wrapper.
//!
//! [`DownstreamChannel<T>`] pairs a downstream [`Transport`] with a
//! receive-accumulation buffer, keeping them together so the gateway can
//! incrementally accumulate response bytes across multiple `recv()` calls.

use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::Transport;

/// A downstream Modbus channel: a transport plus its receive buffer.
///
/// The receive buffer accumulates bytes returned by successive `Transport::recv()`
/// calls until a complete ADU frame can be extracted.
pub struct DownstreamChannel<T: Transport> {
    pub(crate) transport: T,
    pub(crate) rxbuf: Vec<u8, MAX_ADU_FRAME_LEN>,
}

impl<T: Transport> DownstreamChannel<T> {
    /// Wrap a transport in a new downstream channel with an empty receive buffer.
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            rxbuf: Vec::new(),
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

    /// Return whether the underlying transport considers itself connected.
    pub fn is_connected(&self) -> bool {
        self.transport.is_connected()
    }
}
