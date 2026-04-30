//! Bridges [`MbusGatewayCallbacks`] to the
//! [`GatewayEventHandler`](mbus_gateway::GatewayEventHandler) trait expected
//! by [`GatewayServices`](mbus_gateway::GatewayServices).

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::GatewayEventHandler;

use super::callbacks::MbusGatewayCallbacks;

/// Adapter that fans out trait-method calls to the optional C function pointers.
pub struct CGatewayEventAdapter {
    callbacks: MbusGatewayCallbacks,
}

impl CGatewayEventAdapter {
    pub fn new(callbacks: MbusGatewayCallbacks) -> Self {
        Self { callbacks }
    }
}

impl GatewayEventHandler for CGatewayEventAdapter {
    fn on_forward(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr, channel_idx: usize) {
        if let Some(cb) = self.callbacks.on_forward {
            // SAFETY: function pointer + userdata are owned by the caller and
            // remain valid for the lifetime of the gateway instance.
            unsafe {
                cb(
                    session_id,
                    unit.get(),
                    channel_idx as u16,
                    self.callbacks.userdata,
                );
            }
        }
    }

    fn on_response_returned(&mut self, session_id: u8, upstream_txn: u16) {
        if let Some(cb) = self.callbacks.on_response_returned {
            unsafe { cb(session_id, upstream_txn, self.callbacks.userdata) };
        }
    }

    fn on_routing_miss(&mut self, session_id: u8, unit: UnitIdOrSlaveAddr) {
        if let Some(cb) = self.callbacks.on_routing_miss {
            unsafe { cb(session_id, unit.get(), self.callbacks.userdata) };
        }
    }

    fn on_downstream_timeout(&mut self, session_id: u8, internal_txn: u16) {
        if let Some(cb) = self.callbacks.on_downstream_timeout {
            unsafe { cb(session_id, internal_txn, self.callbacks.userdata) };
        }
    }

    fn on_upstream_disconnect(&mut self, session_id: u8) {
        if let Some(cb) = self.callbacks.on_upstream_disconnect {
            unsafe { cb(session_id, self.callbacks.userdata) };
        }
    }
}
