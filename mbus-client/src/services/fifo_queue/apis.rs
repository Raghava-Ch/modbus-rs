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
    /// Sends a Read FIFO Queue request (Function Code 0x18).
    ///
    /// This function allows reading the contents of a remote FIFO queue of registers.
    /// The FIFO structure is address-specific, and the response contains the current
    /// count of registers in the queue followed by the register data itself.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address of the FIFO queue.
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully built, the expectation was queued,
    ///   and the frame was transmitted via the transport layer.
    /// - `Err(MbusError)`: If the address is a broadcast address, if the frame
    ///   construction fails, or if the transport layer fails to send.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_fifo_queue(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        // Modbus protocol specification: Broadcast is not supported for Read operations.
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = fifo_queue::service::ServiceBuilder::read_fifo_queue(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            TRANSPORT::TRANSPORT_TYPE,
        )?;

        // Register an expectation in the client state machine.
        // This ensures that when a response arrives, it is routed to the correct handler.
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Other,
            Self::handle_read_fifo_queue_response,
        )?;

        // Dispatch the compiled ADU frame through the underlying transport (TCP/RTU/ASCII).
        self.dispatch_request_frame(txn_id, unit_id_slave_addr, &frame)?;
        Ok(())
    }
}
