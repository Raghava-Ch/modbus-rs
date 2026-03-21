use heapless::Vec;

use crate::{
    app::RegisterResponse,
    services::register::MAX_REGISTERS_PER_PDU,
    services::{ClientCommon, ClientServices, ExpectedResponse, register},
};
use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::Transport,
};
pub(super) struct ResponseParser;

impl ResponseParser {
    // --- Parsing Methods ---

    /// Parses the response PDU for a Read Holding Registers (FC 0x03) response.
    pub fn parse_read_holding_registers_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadHoldingRegisters,
            expected_quantity,
        )
    }

    /// Parses the response PDU for a Read Input Registers (FC 0x04) response.
    pub(super) fn parse_read_input_registers_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadInputRegisters,
            expected_quantity,
        )
    }

    /// Parses the response PDU for a Write Single Register (FC 0x06) response.
    pub(super) fn parse_write_single_register_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_value: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteSingleRegister {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let value = u16::from_be_bytes([data[2], data[3]]);

        if address != expected_address {
            return Err(MbusError::InvalidAddress);
        }

        if value != expected_value {
            return Err(MbusError::InvalidValue);
        }

        Ok(())
    }

    /// Parses the response PDU for a Write Multiple Registers (FC 0x10) response.
    pub(super) fn parse_write_multiple_registers_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteMultipleRegisters {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);

        if address != expected_address {
            return Err(MbusError::InvalidAddress);
        }

        if quantity != expected_quantity {
            return Err(MbusError::InvalidQuantity);
        }

        Ok(())
    }

    /// Parses the response PDU for a Read/Write Multiple Registers (FC 0x17) response.
    pub(super) fn parse_read_write_multiple_registers_response(
        pdu: &Pdu,
        expected_read_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadWriteMultipleRegisters,
            expected_read_quantity,
        )
    }

    /// Parses the response PDU for a Mask Write Register (FC 0x16) response.
    pub(super) fn parse_mask_write_register_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_and_mask: u16,
        expected_or_mask: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::MaskWriteRegister {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.len() != 6 {
            return Err(MbusError::InvalidDataLen);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let and_mask = u16::from_be_bytes([data[2], data[3]]);
        let or_mask = u16::from_be_bytes([data[4], data[5]]);

        if address != expected_address {
            return Err(MbusError::InvalidAddress);
        }

        if and_mask != expected_and_mask {
            return Err(MbusError::InvalidAndMask);
        }

        if or_mask != expected_or_mask {
            return Err(MbusError::InvalidOrMask);
        }

        Ok(())
    }

    fn parse_read_registers_response(
        pdu: &Pdu,
        expected_fc: FunctionCode,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        if pdu.function_code() != expected_fc {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }

        let byte_count = data[0] as usize;
        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidByteCount);
        }

        if byte_count != (expected_quantity * 2) as usize {
            return Err(MbusError::InvalidQuantity);
        }

        let mut values = Vec::new();
        for chunk in data[1..].chunks(2) {
            if chunk.len() == 2 {
                let val = u16::from_be_bytes([chunk[0], chunk[1]]);
                values
                    .push(val)
                    .map_err(|_| MbusError::BufferLenMissmatch)?;
            }
        }
        Ok(values)
    }
}

impl<T, APP, const N: usize> ClientServices<T, APP, N>
where
    T: Transport,
    APP: ClientCommon + RegisterResponse,
{
    /// Handles a Read Holding Registers response by validating it against the expected response metadata and invoking the appropriate application callback.
    pub(super) fn handle_read_holding_registers_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let from_address = ctx.operation_meta.address();
        let expected_quantity = ctx.operation_meta.quantity();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let register_rsp =
            match register::service::ServiceDecompiler::handle_read_holding_register_rsp(
                pdu,
                expected_quantity,
                from_address,
            ) {
                Ok(register_response) => register_response,
                Err(e) => {
                    self.app
                        .request_failed(transaction_id, unit_id_or_slave_addr, e);
                    return;
                }
            };

        if ctx.operation_meta.is_single() {
            let value = register_rsp.values().get(0).copied().unwrap_or(0);
            self.app.read_single_holding_register_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                value,
            );
        } else {
            self.app.read_multiple_holding_registers_response(
                transaction_id,
                unit_id_or_slave_addr,
                &register_rsp,
            );
        }
    }

    /// Handles a Read Input Registers response by validating it against the expected response metadata and invoking the appropriate application callback.
    pub(super) fn handle_read_input_registers_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let from_address = ctx.operation_meta.address();
        let quantity = ctx.operation_meta.quantity();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let register_rsp =
            match register::service::ServiceDecompiler::handle_read_input_register_rsp(
                pdu,
                quantity,
                from_address,
            ) {
                Ok(register_response) => register_response,
                Err(err) => {
                    self.app
                        .request_failed(transaction_id, unit_id_or_slave_addr, err);
                    return;
                }
            };
        if ctx.operation_meta.is_single() {
            let value = match register_rsp.value(from_address) {
                Ok(v) => v,
                Err(err) => {
                    self.app
                        .request_failed(transaction_id, unit_id_or_slave_addr, err);
                    return;
                }
            };
            self.app.read_single_input_register_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                value,
            );
        } else {
            self.app.read_multiple_input_registers_response(
                transaction_id,
                unit_id_or_slave_addr,
                &register_rsp,
            );
        }
    }

    /// Handles a Write Single Register response by invoking the appropriate application callback.
    pub(super) fn handle_write_single_register_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let address = ctx.operation_meta.address();
        let value = ctx.operation_meta.single_value();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        if register::service::ServiceDecompiler::handle_write_single_register_rsp(
            pdu, address, value,
        )
        .is_ok()
        {
            self.app.write_single_register_response(
                transaction_id,
                unit_id_or_slave_addr,
                address,
                value,
            );
        } else {
            self.app
                .request_failed(transaction_id, unit_id_or_slave_addr, MbusError::ParseError);
        }
    }

    /// Handles a Write Multiple Registers response by invoking the appropriate application callback.
    pub(super) fn handle_write_multiple_registers_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let from_address = ctx.operation_meta.address();
        let quantity = ctx.operation_meta.quantity();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        if register::service::ServiceDecompiler::handle_write_multiple_registers_rsp(
            pdu,
            from_address,
            quantity,
        )
        .is_ok()
        {
            self.app.write_multiple_registers_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                quantity,
            );
        } else {
            self.app
                .request_failed(transaction_id, unit_id_or_slave_addr, MbusError::ParseError);
        }
    }

    pub(super) fn handle_read_write_multiple_registers_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let from_address = ctx.operation_meta.address();
        let read_quantity = ctx.operation_meta.quantity();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let register_rsp =
            match register::service::ServiceDecompiler::handle_read_write_multiple_registers_rsp(
                pdu,
                read_quantity,
                from_address,
            ) {
                Ok(register_response) => register_response,
                Err(e) => {
                    self.app
                        .request_failed(transaction_id, unit_id_or_slave_addr, e);
                    return;
                }
            };

        self.app.read_write_multiple_registers_response(
            transaction_id,
            unit_id_or_slave_addr,
            &register_rsp,
        );
    }

    pub(super) fn handle_mask_write_register_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let ref_address = ctx.operation_meta.address();
        let and_mask = ctx.operation_meta.and_mask();
        let or_mask = ctx.operation_meta.or_mask();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        if register::service::ServiceDecompiler::handle_mask_write_register_rsp(
            pdu,
            ref_address,
            and_mask,
            or_mask,
        )
        .is_ok()
        {
            self.app
                .mask_write_register_response(transaction_id, unit_id_or_slave_addr);
        } else {
            self.app
                .request_failed(transaction_id, unit_id_or_slave_addr, MbusError::ParseError);
        }
    }
}
