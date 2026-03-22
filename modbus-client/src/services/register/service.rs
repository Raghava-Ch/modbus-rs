use heapless::Vec;

use crate::services::register::{Registers, request::ReqPduCompiler, response::ResponseParser};
use mbus_core::{
    data_unit::common::MAX_ADU_FRAME_LEN,
    data_unit::common::{self, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::TransportType,
};

/// Provides operations for creating and parsing Modbus Register request/response PDUs.
#[derive(Debug, Clone)]
pub(super) struct ServiceBuilder;

impl ServiceBuilder {
    /// Sends a Read Holding Registers request.
    pub(super) fn read_holding_registers(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_holding_registers_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Read Input Registers request.
    pub(super) fn read_input_registers(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_input_registers_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Single Register request.
    pub(super) fn write_single_register(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::write_single_register_request(address, value)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Multiple Registers request.
    pub(super) fn write_multiple_registers(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::write_multiple_registers_request(address, quantity, values)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Read/Write Multiple Registers request.
    pub(super) fn read_write_multiple_registers(
        txn_id: u16,
        unit_id: u8,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_write_multiple_registers_request(
            read_address,
            read_quantity,
            write_address,
            write_values,
        )?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Mask Write Register request.
    pub(super) fn mask_write_register(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        and_mask: u16,
        or_mask: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::mask_write_register_request(address, and_mask, or_mask)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }
}

pub(super) struct ServiceDecompiler;

impl ServiceDecompiler {
    /// Handles a Write Single Register response.
    pub(super) fn handle_write_single_register_rsp(
        pdu: &Pdu,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteSingleRegister {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_write_single_register_response(pdu, address, value)
    }

    /// Handles a Read Holding Registers response.
    pub(super) fn handle_read_holding_register_rsp(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if pdu.function_code() != FunctionCode::ReadHoldingRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = ResponseParser::parse_read_holding_registers_response(pdu, expected_quantity)?;
        let registers = Registers::new(from_address, expected_quantity)?
            .with_values(values.as_slice(), values.len() as u16)?;
        Ok(registers)
    }

    /// Handles a Read Input Registers response.
    pub(super) fn handle_read_input_register_rsp(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if pdu.function_code() != FunctionCode::ReadInputRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = ResponseParser::parse_read_input_registers_response(pdu, expected_quantity)?;
        let registers = Registers::new(from_address, expected_quantity)?
            .with_values(values.as_slice(), values.len() as u16)?;
        Ok(registers)
    }

    /// Handles a Write Multiple Registers response.
    pub(super) fn handle_write_multiple_registers_rsp(
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteMultipleRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_write_multiple_registers_response(
            pdu,
            expected_address,
            expected_quantity,
        )
    }

    /// Handles a Read/Write Multiple Registers response.
    pub(super) fn handle_read_write_multiple_registers_rsp(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if pdu.function_code() != FunctionCode::ReadWriteMultipleRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values =
            ResponseParser::parse_read_write_multiple_registers_response(pdu, expected_quantity)?;
        let registers = Registers::new(from_address, expected_quantity)?
            .with_values(values.as_slice(), values.len() as u16)?;
        Ok(registers)
    }

    /// Handles a Mask Write Register response.
    pub(super) fn handle_mask_write_register_rsp(
        pdu: &Pdu,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::MaskWriteRegister {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_mask_write_register_response(pdu, address, and_mask, or_mask)
    }
}
