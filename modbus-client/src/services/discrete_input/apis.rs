use crate::{
    app::DiscreteInputResponse,
    services::{ClientCommon, ClientServices, Multiple, OperationMeta, Single, discrete_input},
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
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address of the inputs to read.
    /// - `quantity`: The number of inputs to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted. Or else Error
    pub fn read_discrete_inputs(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = discrete_input::service::ServiceBuilder::read_discrete_inputs(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Multiple(Multiple {
                address, // Starting address of the read operation
                quantity, // Number of discrete inputs to read
            }),
            Self::handle_read_discrete_inputs_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Discrete Inputs request for a single input.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The exact address of the single input to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    pub fn read_single_discrete_input(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = discrete_input::service::ServiceBuilder::read_discrete_inputs(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            1,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Single(Single {
                address, // Address of the single discrete input
                value: 0, // Value is not relevant for read requests
            }),
            Self::handle_read_discrete_inputs_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }
}
