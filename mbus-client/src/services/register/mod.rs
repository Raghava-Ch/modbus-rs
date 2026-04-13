//! # Modbus Register Service
//!
//! This module implements the client-side logic for Modbus Register operations,
//! supporting both **Holding Registers** and **Input Registers**.
//!
//! ## Supported Function Codes
//! - **FC 03 (0x03)**: Read Holding Registers
//! - **FC 04 (0x04)**: Read Input Registers
//! - **FC 06 (0x06)**: Write Single Register
//! - **FC 16 (0x10)**: Write Multiple Registers
//! - **FC 22 (0x16)**: Mask Write Register
//! - **FC 23 (0x17)**: Read/Write Multiple Registers
//!
//! ## Module Structure
//! - `apis`: High-level public API for the `ClientServices` struct to trigger register operations.
//! - `request`: Handles the construction and serialization of register-related request PDUs.
//! - `response`: Handles parsing, validation, and dispatching of response PDUs to the application.
//! - `service`: Internal orchestration logic for building ADUs and handling de-encapsulation.
//!
//! ## Features
//! - **no_std**: Fully compatible with embedded environments using fixed-size buffers via `heapless`.
//! - **Validation**: Ensures response addresses, quantities, and values match the original request.

pub mod request;
pub mod response;

pub use mbus_core::models::register::*;

mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::services::register::{request::ReqPduCompiler, response::ResponseParser};
    use mbus_core::{
        data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode,
    };

    // --- Read Holding Registers (FC 0x03) ---

    /// Test case: `read_holding_registers_request` with valid parameters.
    #[test]
    fn test_read_holding_registers_request_valid() {
        let pdu = ReqPduCompiler::read_holding_registers_request(0x006B, 3).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x6B, 0x00, 0x03]);
    }

    /// Test case: `read_holding_registers_request` with invalid quantity (too low).
    #[test]
    fn test_read_holding_registers_invalid_quantity() {
        // Quantity 0 is invalid
        assert_eq!(
            ReqPduCompiler::read_holding_registers_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
        // Quantity 126 is invalid (max 125)
        assert_eq!(
            ReqPduCompiler::read_holding_registers_request(0, 126).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_holding_registers_request` with maximum allowed quantity.
    #[test]
    fn test_read_holding_registers_request_max_quantity() {
        let pdu = ReqPduCompiler::read_holding_registers_request(0x0000, 125).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x7D]); // 125 = 0x7D
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Read Input Registers (FC 0x04) ---

    #[test]
    fn test_read_input_registers_request_valid() {
        let pdu = ReqPduCompiler::read_input_registers_request(0x0008, 1).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadInputRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x08, 0x00, 0x01]);
    }

    /// Test case: `read_input_registers_request` with invalid quantity (too low).
    #[test]
    fn test_read_input_registers_request_invalid_quantity_low() {
        assert_eq!(
            ReqPduCompiler::read_input_registers_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_input_registers_request` with invalid quantity (too high).
    #[test]
    fn test_read_input_registers_request_invalid_quantity_high() {
        assert_eq!(
            ReqPduCompiler::read_input_registers_request(0, 126).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_input_registers_request` with maximum allowed quantity.
    #[test]
    fn test_read_input_registers_request_max_quantity() {
        let pdu = ReqPduCompiler::read_input_registers_request(0x0000, 125).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadInputRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x7D]); // 125 = 0x7D
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Write Single Register (FC 0x06) ---

    #[test]
    fn test_write_single_register_request_valid() {
        let pdu = ReqPduCompiler::write_single_register_request(0x0001, 0x0003).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleRegister);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x01, 0x00, 0x03]);
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Write Multiple Registers (FC 0x10) ---

    /// Test case: `write_multiple_registers_request` with valid data.
    #[test]
    fn test_write_multiple_registers_request_valid() {
        let quantity = 2;
        let values = [0x0001, 0x0002];
        let pdu =
            ReqPduCompiler::write_multiple_registers_request(0x0000, quantity, &values).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleRegisters);
        assert_eq!(
            pdu.data().as_slice(),
            &[0x00, 0x00, 0x00, 0x02, 0x04, 0x00, 0x01, 0x00, 0x02]
        );
        assert_eq!(pdu.data_len(), 9); // 2 addr + 2 qty + 1 byte_count + 4 data
    }

    /// Test case: `write_multiple_registers_request` with invalid quantity (too low).
    #[test]
    fn test_write_multiple_registers_request_invalid_quantity_low() {
        let values: [u16; 0] = [];
        assert_eq!(
            ReqPduCompiler::write_multiple_registers_request(0x0000, 0, &values).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `write_multiple_registers_request` with invalid quantity (too high).
    #[test]
    fn test_write_multiple_registers_request_invalid_quantity_high() {
        let values = [0x0000; 124]; // Max is 123
        assert_eq!(
            ReqPduCompiler::write_multiple_registers_request(0x0000, 124, &values).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `write_multiple_registers_request` returns an error for quantity-values mismatch.
    #[test]
    fn test_write_multiple_registers_request_quantity_values_mismatch() {
        let values = [0x1234, 0x5678];
        let result = ReqPduCompiler::write_multiple_registers_request(0x0001, 3, &values); // Quantity 3, but only 2 values
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_registers_request` with maximum allowed quantity.
    #[test]
    fn test_write_multiple_registers_request_max_quantity() {
        let values = [0x0000; 123]; // Max is 123
        let pdu = ReqPduCompiler::write_multiple_registers_request(0x0000, 123, &values).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleRegisters);
        assert_eq!(pdu.data_len(), 5 + (123 * 2)); // 5 + 246 = 251
    }

    // --- Read/Write Multiple Registers (FC 0x17) ---

    /// Test case: `read_write_multiple_registers_request` with valid data.
    #[test]
    fn test_read_write_multiple_registers_request_valid() {
        let write_values = [0x0001, 0x0002];
        let pdu =
            ReqPduCompiler::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::ReadWriteMultipleRegisters
        );
        assert_eq!(
            pdu.data().as_slice(),
            &[
                0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x01, 0x00, 0x02
            ]
        );
        assert_eq!(pdu.data_len(), 13); // 2 read_addr + 2 read_qty + 2 write_addr + 2 write_qty + 1 byte_count + 4 write_values
    }

    /// Test case: `read_write_multiple_registers_request` with invalid read quantity (too low).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_read_quantity_low() {
        let write_values = [0x0001];
        assert_eq!(
            ReqPduCompiler::read_write_multiple_registers_request(0x0000, 0, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid read quantity (too high).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_read_quantity_high() {
        let write_values = [0x0001];
        assert_eq!(
            ReqPduCompiler::read_write_multiple_registers_request(
                0x0000,
                126,
                0x0001,
                &write_values
            )
            .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid write quantity (too low).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_write_quantity_low() {
        let write_values: [u16; 0] = [];
        assert_eq!(
            ReqPduCompiler::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid write quantity (too high).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_write_quantity_high() {
        let write_values = [0x0000; 122]; // Max is 121
        assert_eq!(
            ReqPduCompiler::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with maximum allowed read and write quantities.
    #[test]
    fn test_read_write_multiple_registers_request_max_quantities() {
        let write_values = [0x0000; 121]; // Max write quantity
        let pdu = ReqPduCompiler::read_write_multiple_registers_request(
            0x0000,
            125,
            0x0001,
            &write_values,
        )
        .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::ReadWriteMultipleRegisters
        );
        assert_eq!(pdu.data_len(), 9 + (121 * 2)); // 9 + 242 = 251
    }

    // --- Mask Write Register (FC 0x16) ---

    /// Test case: `mask_write_register_request` with valid data.
    #[test]
    fn test_mask_write_register_request_valid() {
        let pdu = ReqPduCompiler::mask_write_register_request(0x0004, 0xF002, 0x0025).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::MaskWriteRegister);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x04, 0xF0, 0x02, 0x00, 0x25]);
        assert_eq!(pdu.data_len(), 6);
    }

    // --- Parse Read/Write Multiple Registers Response Tests ---

    /// Test case: `parse_read_write_multiple_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_read_write_multiple_registers_response_valid() {
        // Response for reading 2 registers: FC(0x17), Byte Count(0x04), Data(0x1234, 0x5678)
        let response_bytes = [0x17, 0x04, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers =
            ResponseParser::parse_read_write_multiple_registers_response(&pdu, 2).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_write_multiple_registers_response_wrong_fc() {
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for empty data.
    #[test]
    fn test_parse_read_write_multiple_registers_response_empty_data() {
        let pdu = Pdu::new(FunctionCode::ReadWriteMultipleRegisters, Vec::new(), 0);
        assert_eq!(
            ResponseParser::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_write_multiple_registers_response_byte_count_mismatch() {
        let response_bytes = [0x17, 0x02, 0x12, 0x34, 0x56, 0x78]; // Byte count 2, but 4 data bytes
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidByteCount
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for expected quantity mismatch.
    #[test]
    fn test_parse_read_write_multiple_registers_response_expected_quantity_mismatch() {
        let response_bytes = [0x17, 0x04, 0x12, 0x34, 0x56, 0x78]; // 2 registers in response
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_write_multiple_registers_response(&pdu, 3).unwrap_err(),
            MbusError::InvalidQuantity
        ); // Expected 3, got 2
    }

    // --- Parse Mask Write Register Response Tests ---

    /// Test case: `parse_mask_write_register_response` successfully parses a valid response.
    #[test]
    fn test_parse_mask_write_register_response_valid() {
        // Response: FC(0x16), Address(0x0004), AND Mask(0xF002), OR Mask(0x0025)
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x25];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .is_ok()
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for wrong function code.
    #[test]
    fn test_parse_mask_write_register_response_wrong_fc() {
        let response_bytes = [0x06, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x25]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_mask_write_register_response_invalid_len() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for address mismatch.
    #[test]
    fn test_parse_mask_write_register_response_address_mismatch() {
        let response_bytes = [0x16, 0x00, 0x05, 0xF0, 0x02, 0x00, 0x25]; // Address mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidAddress
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for AND mask mismatch.
    #[test]
    fn test_parse_mask_write_register_response_and_mask_mismatch() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x01, 0x00, 0x25]; // AND mask mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidAndMask
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for OR mask mismatch.
    #[test]
    fn test_parse_mask_write_register_response_or_mask_mismatch() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x26]; // OR mask mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidOrMask
        );
    }

    // --- Parse Read Holding Registers Response Tests ---

    /// Test case: `parse_read_holding_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_read_holding_registers_response_valid() {
        // Response for reading 2 registers: FC(0x03), Byte Count(0x04), Data(0x1234, 0x5678)
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers = ResponseParser::parse_read_holding_registers_response(&pdu, 2).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_holding_registers_response_wrong_fc() {
        let response_bytes = [0x04, 0x04, 0x12, 0x34, 0x56, 0x78]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_holding_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_holding_registers_response_byte_count_mismatch() {
        let response_bytes = [0x03, 0x03, 0x12, 0x34, 0x56, 0x78]; // Byte count 3, but 4 data bytes
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_holding_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidByteCount
        );
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for expected quantity mismatch.
    #[test]
    fn test_parse_read_holding_registers_response_expected_quantity_mismatch() {
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78]; // 2 registers in response
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_holding_registers_response(&pdu, 3).unwrap_err(),
            MbusError::InvalidQuantity
        ); // Expected 3, got 2
    }

    // --- Parse Write Single Register Response Tests ---

    /// Test case: `parse_write_single_register_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_single_register_response_valid() {
        let response_bytes = [0x06, 0x00, 0x01, 0x12, 0x34]; // FC, Address, Value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(ResponseParser::parse_write_single_register_response(&pdu, 0x0001, 0x1234).is_ok());
    }

    /// Test case: `parse_write_single_register_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_single_register_response_address_mismatch() {
        let response_bytes = [0x06, 0x00, 0x02, 0x12, 0x34];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_write_single_register_response(&pdu, 0x0001, 0x1234).unwrap_err(),
            MbusError::InvalidAddress
        );
    }

    /// Test case: `parse_write_single_register_response` returns an error for value mismatch.
    #[test]
    fn test_parse_write_single_register_response_value_mismatch() {
        let response_bytes = [0x06, 0x00, 0x01, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_write_single_register_response(&pdu, 0x0001, 0x1234).unwrap_err(),
            MbusError::InvalidValue
        );
    }

    // --- Parse Write Multiple Registers Response Tests ---

    /// Test case: `parse_write_multiple_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_multiple_registers_response_valid() {
        let response_bytes = [0x10, 0x00, 0x01, 0x00, 0x02]; // FC, Address, Quantity
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(ResponseParser::parse_write_multiple_registers_response(&pdu, 0x0001, 2).is_ok());
    }

    /// Test case: `parse_write_multiple_registers_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_multiple_registers_response_address_mismatch() {
        let response_bytes = [0x10, 0x00, 0x02, 0x00, 0x02];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_write_multiple_registers_response(&pdu, 0x0001, 2).unwrap_err(),
            MbusError::InvalidAddress
        );
    }

    /// Test case: `parse_write_multiple_registers_response` returns an error for quantity mismatch.
    #[test]
    fn test_parse_write_multiple_registers_response_quantity_mismatch() {
        let response_bytes = [0x10, 0x00, 0x01, 0x00, 0x03];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_write_multiple_registers_response(&pdu, 0x0001, 2).unwrap_err(),
            MbusError::InvalidQuantity
        );
    }
}
