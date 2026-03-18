use crate::{
    app::DiscreteInputResponse,
    services::{
        ClientCommon, ClientServices, ExpectedResponse, Multiple, OperationMeta, Single,
        discrete_input,
    },
};
use mbus_core::{
    errors::MbusError,
    transport::{Transport, UnitIdOrSlaveAddr},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + DiscreteInputResponse,
{
    /// Sends a Read Discrete Inputs request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the inputs to read.
    /// - `quantity`: The number of inputs to read.
    pub fn read_discrete_inputs(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = discrete_input::service::ServiceBuilder::read_discrete_inputs(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_discrete_inputs_response,
                operation_meta: OperationMeta::Multiple(Multiple {
                    address: address,
                    quantity: quantity,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Discrete Inputs request for a single input.
    ///
    pub fn read_single_discrete_input(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        let frame = discrete_input::service::ServiceBuilder::read_discrete_inputs(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            1,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_discrete_inputs_response,
                operation_meta: OperationMeta::Single(Single {
                    address: address,
                    value: 0,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }
}
