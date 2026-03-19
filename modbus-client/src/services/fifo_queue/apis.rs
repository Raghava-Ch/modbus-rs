use crate::app::FifoQueueResponse;
use crate::services::{ClientCommon, ClientServices, OperationMeta, fifo_queue};
use mbus_core::{
    errors::MbusError,
    transport::{Transport, UnitIdOrSlaveAddr},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + FifoQueueResponse,
{
    /// Sends a Read FIFO Queue request.
    pub fn read_fifo_queue(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = fifo_queue::service::ServiceBuilder::read_fifo_queue(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_read_fifo_queue_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
