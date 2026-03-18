use mbus_core::{
    errors::MbusError,
    transport::{Transport, UnitIdOrSlaveAddr},
};

use crate::{
    app::CoilResponse,
    services::{
        ClientCommon, ClientServices, ExpectedResponse, Multiple, OperationMeta, Single, coil,
    },
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + CoilResponse,
{
    /// Sends a Read Coils request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the coils to read.
    /// - `quantity`: The number of coils to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn read_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let frame = coil::service::ServiceBuilder::read_coils(
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
                handler: Self::handle_read_coils_response,
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

    /// Sends a Read Single Coil request to the specified unit ID and address, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the coil to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    ///
    /// This method is a convenience wrapper around `read_multiple_coils` for reading a single coil, which simplifies the application logic when only one coil needs to be read.
    pub fn read_single_coil(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame =
            coil::service::ServiceBuilder::read_coils(txn_id, unit_id_slave_addr.get(), address, 1, transport_type)?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_read_coils_response,
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

    /// Sends a Write Single Coil request to the specified unit ID and address with the given value, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the coil to write.
    /// - `value`: The boolean value to write to the coil (true for ON, false for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_single_coil(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let frame = coil::service::ServiceBuilder::write_single_coil(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            value,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_write_single_coil_response,
                operation_meta: OperationMeta::Single(Single {
                    address: address,
                    value: value as u16,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write Multiple Coils request to the specified unit ID and address with the given values, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the coils to write.
    /// - `quantity`: The number of coils to write.
    /// - `values`: A slice of boolean values to write to the coils (true for ON, false for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error constructing the request or sending it.
    pub fn write_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        values: &[bool],
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly
        let frame = coil::service::ServiceBuilder::write_multiple_coils(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            quantity,
            values,
            transport_type,
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),
                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.retry_attempts(),
                handler: Self::handle_write_multiple_coils_response,
                operation_meta: OperationMeta::Multiple(Multiple { address, quantity }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
