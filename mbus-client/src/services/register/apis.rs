use crate::app::RegisterResponse;
use crate::services::{
    ClientCommon, ClientServices, Mask, Multiple, OperationMeta, Single, register,
};
use mbus_core::{
    errors::MbusError,
    transport::{Transport, UnitIdOrSlaveAddr},
};

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: RegisterResponse + ClientCommon,
{
    /// Sends a Read Holding Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `from_address`: The starting address of the holding registers to read.
    /// - `quantity`: The number of holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_holding_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        from_address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_holding_registers(
            txn_id,
            unit_id_slave_addr.get(),
            from_address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Multiple(Multiple {
                address: from_address, // Starting address of the read operation
                quantity,              // Number of registers to read
            }),
            Self::handle_read_holding_registers_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Holding Registers request for a single register (Function Code 0x03).
    ///
    /// This is a convenience wrapper around `read_holding_registers` with a quantity of 1.
    /// It allows the application to receive a simplified `read_single_holding_register_response`
    /// callback instead of handling a register collection.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address of the holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_single_holding_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        use crate::services::Single;

        // Modbus protocol specification: Broadcast is not supported for Read operations.
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        // Construct the ADU frame using the register service builder with quantity = 1
        let frame = register::service::ServiceBuilder::read_holding_registers(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            1, // quantity = 1
            self.transport.transport_type(),
        )?;

        // Register an expectation. We use OperationMeta::Single to signal the response
        // handler to trigger the single-register specific callback in the app layer.
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Single(Single {
                address,  // Address of the single register
                value: 0, // Value is not relevant for read requests
            }),
            Self::handle_read_holding_registers_response,
        )?;

        // Dispatch the compiled frame through the underlying transport.
        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Input Registers request (Function Code 0x04).
    ///
    /// This function is used to read from 1 to 125 contiguous input registers in a remote device.
    /// Input registers are typically used for read-only data like sensor readings.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address of the input registers to read (0x0000 to 0xFFFF).
    /// - `quantity`: The number of input registers to read (1 to 125).
    ///
    /// # Returns
    /// - `Ok(())`: If the request was successfully built, the expectation was queued,
    ///   and the frame was transmitted.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_input_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_input_registers(
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
                address,  // Starting address of the read operation
                quantity, // Number of registers to read
            }),
            Self::handle_read_input_registers_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Input Registers request for a single register (Function Code 0x04).
    ///
    /// This is a convenience wrapper around `read_input_registers` with a quantity of 1.
    /// It allows the application to receive a simplified `read_single_input_register_response`
    /// callback instead of handling a register collection.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The exact address of the input register to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to read from a broadcast address.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_single_input_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_input_registers(
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
                address,  // Address of the single register
                value: 0, // Value is not relevant for read requests
            }),
            Self::handle_read_input_registers_response,
        )?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Write Single Register request (Function Code 0x06).
    ///
    /// This function is used to write a single holding register in a remote device.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the holding register to be written.
    /// - `value`: The 16-bit value to be written to the register.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Broadcast Support
    /// Serial Modbus (RTU/ASCII) allows broadcast writes (Slave Address 0). In this case,
    /// the request is sent to all slaves, and no response is expected or queued.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to broadcast over TCP.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn write_single_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = register::service::ServiceBuilder::write_single_register(
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
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Single(Single { address, value }),
                Self::handle_write_single_register_response, // Callback for successful response
            )?; // Expect a response for non-broadcast writes
        }

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write Multiple Registers request (Function Code 0x10).
    ///
    /// This function is used to write a block of contiguous registers (1 to 123 registers)
    /// in a remote device.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `quantity`: The number of registers to write (1 to 123).
    /// - `values`: A slice of `u16` values to be written. The length must match `quantity`.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    ///
    /// # Broadcast Support
    /// Serial Modbus allows broadcast. No response is expected for broadcast requests.
    ///
    /// # Errors
    /// Returns `Err(MbusError::BroadcastNotAllowed)` if attempting to broadcast over TCP.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn write_multiple_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        let transport_type = self.transport.transport_type();
        let frame = register::service::ServiceBuilder::write_multiple_registers(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            quantity,
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
                OperationMeta::Multiple(Multiple { address, quantity }),
                Self::handle_write_multiple_registers_response, // Callback for successful response
            )?; // Expect a response for non-broadcast writes
        }

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Read/Write Multiple Registers request (FC 23).
    ///
    /// This function performs a combination of one read operation and one write operation in a single
    /// Modbus transaction. The write operation is performed before the read.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `read_address`: The starting address of the registers to read.
    /// - `read_quantity`: The number of registers to read.
    /// - `write_address`: The starting address of the registers to write.
    /// - `write_values`: A slice of `u16` values to be written to the device.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error
    /// constructing the request (e.g., invalid quantity) or sending it over the transport.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn read_write_multiple_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BroadcastNotAllowed); // FC 23 explicitly forbids broadcast
        }

        // 1. Construct the ADU frame using the register service
        let transport_type = self.transport.transport_type();
        let frame = register::service::ServiceBuilder::read_write_multiple_registers(
            txn_id,
            unit_id_slave_addr.get(),
            read_address,
            read_quantity,
            write_address,
            write_values,
            transport_type,
        )?;

        // 2. Queue the expected response to match against the incoming server reply
        self.add_an_expectation(
            txn_id,
            unit_id_slave_addr,
            &frame,
            OperationMeta::Multiple(Multiple {
                address: read_address,   // Starting address of the read operation
                quantity: read_quantity, // Number of registers to read
            }),
            Self::handle_read_write_multiple_registers_response,
        )?;

        // 3. Transmit the frame via the configured transport
        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Mask Write Register request.
    ///
    /// This function is used to modify the contents of a single holding register using a combination
    /// of an AND mask and an OR mask. The new value of the register is calculated as:
    /// `(current_value AND and_mask) OR (or_mask AND (NOT and_mask))`
    ///
    /// The request is added to the `expected_responses` queue to await a corresponding reply from the Modbus server.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the register to apply the mask to.
    /// - `and_mask`: The 16-bit AND mask to apply to the current register value.
    /// - `or_mask`: The 16-bit OR mask to apply to the current register value.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent and queued for a response,
    /// or an `MbusError` if there was an error during request construction,
    /// sending over the transport, or if the `expected_responses` queue is full.
    #[must_use = "request submission errors should be handled; the request may not have been queued/sent"]
    pub fn mask_write_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        let frame = register::service::ServiceBuilder::mask_write_register(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            and_mask,
            or_mask,
            self.transport.transport_type(),
        )?;

        if unit_id_slave_addr.is_broadcast() {
            if self.transport.transport_type().is_tcp_type() {
                return Err(MbusError::BroadcastNotAllowed);
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Masking(Mask {
                    address,  // Address of the register to mask
                    and_mask, // AND mask used in the request
                    or_mask,  // OR mask used in the request
                }),
                Self::handle_mask_write_register_response,
            )?;
        }

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }
}
