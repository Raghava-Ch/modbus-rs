use crate::app::RegisterResponse;
use crate::services::{
    ClientCommon, ClientServices, ExpectedResponse, Mask, Multiple, OperationMeta, Single, register,
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
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the holding registers to read.
    /// - `quantity`: The number of holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    pub fn read_holding_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        from_address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_holding_registers(
            txn_id,
            unit_id_slave_addr.get(),
            from_address,
            quantity,
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),

                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.config.retry_attempts(),

                handler: Self::handle_read_holding_registers_response,

                operation_meta: OperationMeta::Multiple(Multiple {
                    address: from_address,
                    quantity: quantity,
                }),
            })
            .map_err(|_| MbusError::TooManyRequests)?;

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;

        Ok(())
    }

    /// Sends a Read Holding Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the holding registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    pub fn read_single_holding_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        use crate::services::Single;

        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_holding_registers(
            txn_id,
            unit_id_slave_addr.get(),
            address,
            1, // quantity = 1
            self.transport.transport_type(),
        )?;

        self.expected_responses
            .push(ExpectedResponse {
                txn_id,
                unit_id_or_slave_addr: unit_id_slave_addr.get(),

                original_adu: frame.clone(),
                sent_timestamp: self.app.current_millis(),
                retries_left: self.config.retry_attempts(),

                handler: Self::handle_read_holding_registers_response,

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

    /// Sends a Read Input Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the input registers to read.
    /// - `quantity`: The number of input registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    pub fn read_input_registers(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_input_registers(
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
                retries_left: self.config.retry_attempts(),

                handler: Self::handle_read_input_registers_response,

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

    /// Sends a Read Input Registers request to the specified unit ID and address range, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the input registers to read.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to read from address `0` (Broadcast).
    pub fn read_single_input_register(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
    ) -> Result<(), MbusError> {
        if unit_id_slave_addr.is_broadcast() {
            return Err(MbusError::BoradcastNotAllowed); // Modbus forbids broadcast Read operations
        }

        let frame = register::service::ServiceBuilder::read_input_registers(
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
                retries_left: self.config.retry_attempts(),

                handler: Self::handle_read_input_registers_response,

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

    /// Sends a Write Single Register request to the specified unit ID and address with the given value, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the register to write.
    /// - `value`: The `u16` value to write to the register.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to broadcast over TCP.
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
                return Err(MbusError::BoradcastNotAllowed); // Modbus TCP typically does not support broadcast
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Single(Single { address, value }),
                Self::handle_write_single_register_response,
            )?;
        }

        self.transport
            .send(&frame)
            .map_err(|_e| MbusError::SendFailed)?;
        Ok(())
    }

    /// Sends a Write Multiple Registers request to the specified unit ID and address with the given values, and records the expected response.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The starting address of the registers to write.
    /// - `quantity`: The number of registers to write.
    /// - `values`: A slice of `u16` values to write to the registers.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully enqueued and transmitted.
    /// 
    /// # Errors
    /// Returns `Err(MbusError::BoradcastNotAllowed)` if attempting to broadcast over TCP.
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
                return Err(MbusError::BoradcastNotAllowed); // Modbus TCP typically does not support broadcast
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Multiple(Multiple { address, quantity }),
                Self::handle_write_multiple_registers_response,
            )?;
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
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `read_address`: The starting address of the registers to read.
    /// - `read_quantity`: The number of registers to read.
    /// - `write_address`: The starting address of the registers to write.
    /// - `write_values`: A slice of `u16` values to be written to the device.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent, or an `MbusError` if there was an error
    /// constructing the request (e.g., invalid quantity) or sending it over the transport.
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
            return Err(MbusError::BoradcastNotAllowed); // FC 23 explicitly forbids broadcast
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
                address: read_address,
                quantity: read_quantity,
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
    /// - `txn_id`: The transaction ID for this request, used to match responses.
    /// - `unit_id`: The Modbus unit ID of the target device.
    /// - `address`: The address of the register to apply the mask to.
    /// - `and_mask`: The 16-bit AND mask to apply to the current register value.
    /// - `or_mask`: The 16-bit OR mask to apply to the current register value.
    ///
    /// # Returns
    /// `Ok(())` if the request was successfully sent and queued for a response,
    /// or an `MbusError` if there was an error during request construction,
    /// sending over the transport, or if the `expected_responses` queue is full.
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
                return Err(MbusError::BoradcastNotAllowed);
            }
        } else {
            self.add_an_expectation(
                txn_id,
                unit_id_slave_addr,
                &frame,
                OperationMeta::Masking(Mask {
                    address,
                    and_mask,
                    or_mask,
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
