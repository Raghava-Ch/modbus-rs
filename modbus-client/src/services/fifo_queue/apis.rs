use crate::app::FifoQueueResponse;
use crate::services::{ClientCommon, ClientServices, ExpectedResponse, OperationMeta, fifo_queue};
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
        let frame = fifo_queue::service::ServiceBuilder::read_fifo_queue(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_fifo_queue_response,
                operation_meta: OperationMeta::Other,
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
