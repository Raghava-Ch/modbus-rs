use mbus_core::{
    errors::MbusError,
    models::coil::Coils,
    transport::{Transport, UnitIdOrSlaveAddr},
};

use crate::{
    app::CoilResponse,
    services::{ClientCommon, ClientServices, Multiple, OperationMeta, Single, coil},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + CoilResponse,
{
    /// Sends a Read Coils request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `address`: The starting address of the coils to read.
    /// - `quantity`: The number of coils to read.
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully compiled, registered in the expectation queue, and sent.
    /// - `Err(MbusError)`: If validation fails (e.g., broadcast read), the PDU is invalid, or transport fails.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        // 1. Compile the ADU frame (PDU + Transport Header/Footer)
        // Traces to: coil::service::ServiceBuilder -> ReqPduCompiler::read_coils_request
        let frame = coil::service::ServiceBuilder::read_coils(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            quantity,
            self.transport.transport_type(),
        )?;

        // 2. Register the request in the expectation manager to handle the incoming response
        // Traces to: ClientServices::add_an_expectation
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Multiple(Multiple {
                address,  // Starting address of the read operation
                quantity, // Number of coils to read
            }),
            Self::handle_read_coils_response,
        )?;

        // 3. Dispatch the raw bytes to the physical/network layer
        self.dispatch_request_frame(txn_id, unit_id_slave_addr, &frame)?;

        Ok(())
    }

    /// Sends a Read Single Coil request to the specified unit ID and address, and records the expected response.
    /// This method is a convenience wrapper around `read_multiple_coils` for
    /// reading a single coil, which simplifies the application logic when only one coil needs to be read.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `address`: The address of the coil to read.
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully compiled, registered in the expectation queue, and sent.
    /// - `Err(MbusError)`: If validation fails (e.g., broadcast read), the PDU is invalid, or transport fails.
    ///
    /// Note: This uses FC 0x01 with a quantity of 1.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_single_coil(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        // Traces to: coil::service::ServiceBuilder -> ReqPduCompiler::read_coils_request (qty=1)
        let transport_type = self.transport.transport_type();
        let frame = coil::service::ServiceBuilder::read_coils(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            1,
            transport_type,
        )?;

        // Uses OperationMeta::Single to trigger handle_read_coils_response's single-coil logic
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Single(Single {
                address,  // Address of the single coil
                value: 0, // Value is not relevant for read requests
            }),
            Self::handle_read_coils_response,
        )?;

        self.dispatch_request_frame(txn_id, unit_id_slave_addr, &frame)?;

        Ok(())
    }

    /// Sends a Write Single Coil request to the specified unit ID and address with the given value, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `address`: The address of the coil to write.
    /// - `value`: The boolean value to write to the coil (true for ON, false for OFF).
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully compiled, registered in the expectation queue, and sent.
    /// - `Err(MbusError)`: If validation fails (e.g., broadcast read), the PDU is invalid, or transport fails.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn write_single_coil(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly

        // Traces to: coil::service::ServiceBuilder -> ReqPduCompiler::write_single_coil_request
        let frame = coil::service::ServiceBuilder::write_single_coil(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            value,
            transport_type,
        )?;

        // Modbus TCP typically does not support broadcast.
        // Serial Modbus (RTU/ASCII) allows broadcast writes, but the client MUST NOT
        // expect a response from the server(s).
        if unit_id_slave_addr.is_broadcast() {
            if transport_type.is_tcp_type() {
                return Err(MbusError::BroadcastNotAllowed); // Modbus TCP typically does not support broadcast
            }
        } else {
            // Only add expectation if not a broadcast; servers do not respond to broadcast writes
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Single(Single {
                    address,             // Address of the coil
                    value: value as u16, // Value written (0x0000 or 0xFF00)
                }),
                Self::handle_write_single_coil_response,
            )?;
        }

        self.dispatch_request_frame(txn_id, unit_id_slave_addr, &frame)?;
        Ok(())
    }

    /// Sends a Write Multiple Coils request to the specified unit ID and address with the given values, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `address`: The starting address of the coils to write.
    /// - `quantity`: The number of coils to write.
    /// - `values`: A slice of boolean values to write to the coils (true for ON, false for OFF).
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully compiled, registered in the expectation queue, and sent.
    /// - `Err(MbusError)`: If validation fails (e.g., broadcast read), the PDU is invalid, or transport fails.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn write_multiple_coils(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        values: &Coils,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type(); // Access self.transport directly

        // Traces to: coil::service::ServiceBuilder -> ReqPduCompiler::write_multiple_coils_request
        let frame = coil::service::ServiceBuilder::write_multiple_coils(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            values.quantity(),
            values,
            transport_type,
        )?;

        // Modbus TCP typically does not support broadcast.
        // Serial Modbus (RTU/ASCII) allows broadcast writes, but the client MUST NOT
        // expect a response from the server(s).
        if unit_id_slave_addr.is_broadcast() {
            if transport_type.is_tcp_type() {
                return Err(MbusError::BroadcastNotAllowed); // Modbus TCP typically does not support broadcast
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Multiple(Multiple {
                    address,                     // Starting address of the coils
                    quantity: values.quantity(), // Number of coils written
                }),
                Self::handle_write_multiple_coils_response,
            )?;
        }

        self.dispatch_request_frame(txn_id, unit_id_slave_addr, &frame)?;
        Ok(())
    }
}
