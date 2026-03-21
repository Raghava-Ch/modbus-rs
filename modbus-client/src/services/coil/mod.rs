//! # Modbus Coils Client Services
//!
//! This module provides the orchestration, API surface, and payload parsing required
//! to execute Modbus coil operations over a network transport.
//!
//! ## Supported Function Codes
//! - **Read Coils (FC 0x01)**: Retrieve the ON/OFF status of one or multiple discrete coils.
//! - **Write Single Coil (FC 0x05)**: Force a single coil to `ON` or `OFF`.
//! - **Write Multiple Coils (FC 0x0F)**: Force a continuous block of coils to specific states.
//!
//! It re-exports the fundamental [`Coils`] data model used to interact with the packed bit states.

pub mod request;
pub mod response;

pub use mbus_core::models::coil::*;

mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;
    use mbus_core::models::coil::Coils;

    use crate::services::coil::request::ReqPduCompiler;
    use crate::services::coil::response::ResponseParser;
    use crate::services::coil::{MAX_COIL_BYTES, MAX_COILS_PER_PDU};
    use mbus_core::data_unit::common::Pdu;
    use mbus_core::errors::MbusError;
    use mbus_core::function_codes::public::FunctionCode;

    // --- Read Coils Request Tests ---

    /// Test case: `read_coils_request` creates a valid PDU for reading coils.
    #[test]
    fn test_read_coils_request_valid() {
        let address = 0x0001;
        let quantity = 0x000A; // 10 coils
        let pdu = ReqPduCompiler::read_coils_request(address, quantity).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::ReadCoils);
        assert_eq!(pdu.data_len(), 4);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x01, 0x00, 0x0A]);
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too low).
    #[test]
    fn test_read_coils_request_invalid_quantity_low() {
        let result = ReqPduCompiler::read_coils_request(0x0001, 0);
        assert_eq!(result.unwrap_err(), MbusError::InvalidQuantity);
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too high).
    #[test]
    fn test_read_coils_request_invalid_quantity_high() {
        let result = ReqPduCompiler::read_coils_request(0x0001, 2001);
        assert_eq!(result.unwrap_err(), MbusError::InvalidQuantity);
    }

    // --- Parse Read Coils Response Tests ---

    /// Test case: `parse_read_coils_response` successfully parses a valid response for 8 coils.
    #[test]
    fn test_parse_read_coils_response_valid_8_coils() {
        // Response for reading 8 coils, values: 10110011 (0xB3)
        // PDU: FC (0x01), Byte Count (0x01), Coil Data (0xB3)
        let response_bytes = [0x01, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils_data = ResponseParser::parse_read_coils_response(&pdu, 8).unwrap();

        // The function returns a Vec<u8> containing the raw coil data bytes.
        // For 8 coils with value 0xB3, the expected data is just [0xB3].
        assert_eq!(coils_data.as_slice(), &[0xB3]);
        assert_eq!(coils_data.len(), 1); // One byte of data
    }

    /// Test case: `parse_read_coils_response` successfully parses a valid response for 10 coils.
    #[test]
    fn test_parse_read_coils_response_valid_10_coils() {
        // Response for reading 10 coils.
        // Coil values: 10110011 (0xB3) for the first 8, and 00000011 (0x03) for the next 2.
        // PDU: FC (0x01), Byte Count (0x02), Coil Data (0xB3, 0x03)
        let response_bytes = [0x01, 0x02, 0xB3, 0x03];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils_data = ResponseParser::parse_read_coils_response(&pdu, 10).unwrap();

        // The function returns a Vec<u8> containing the raw coil data bytes.
        // For 10 coils, 2 bytes are expected: [0xB3, 0x03].
        assert_eq!(coils_data.as_slice(), &[0xB3, 0x03]);
        assert_eq!(coils_data.len(), 2); // Two bytes of data
    }

    /// Test case: `parse_read_coils_response` successfully parses a valid response for a quantity that results in a partial last byte.
    #[test]
    fn test_parse_read_coils_response_valid_partial_last_byte() {
        // Reading 3 coils, values: 101 (0x05)
        // PDU: FC (0x01), Byte Count (0x01), Coil Data (0x05)
        let response_bytes = [0x01, 0x01, 0x05];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils_data = ResponseParser::parse_read_coils_response(&pdu, 3).unwrap();

        assert_eq!(coils_data.as_slice(), &[0x05]);
        assert_eq!(coils_data.len(), 1);
    }

    /// Test case: `parse_read_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_read_coils_response_wrong_fc() {
        // PDU with FC 0x03 (Read Holding Registers) instead of 0x01 (Read Coils)
        let response_bytes = [0x03, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }

    /// Test case: `parse_read_coils_response` returns an error for an empty data slice (PDU only contains FC).
    #[test]
    fn test_parse_read_coils_response_empty_data() {
        // PDU: FC (0x01) only, no byte count or coil data
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = ResponseParser::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidDataLen);
    }

    /// Test case: `parse_read_coils_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_coils_response_byte_count_mismatch() {
        // PDU: FC (0x01), Byte Count (0x01), but provides two data bytes (0xB3, 0x00)
        let response_bytes = [0x01, 0x01, 0xB3, 0x00];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidByteCount);
    }

    /// Test case: `parse_read_coils_response` returns an error for expected quantity mismatch with actual byte count.
    #[test]
    fn test_parse_read_coils_response_expected_quantity_mismatch() {
        // PDU: FC (0x01), Byte Count (0x01), Coil Data (0xB3) -> implies 8 coils
        // But `expected_quantity` is 16, which would require 2 bytes of coil data.
        let response_bytes = [0x01, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_read_coils_response(&pdu, 16);
        assert_eq!(result.unwrap_err(), MbusError::InvalidQuantity);
    }

    /// Test case: `parse_read_coils_response` handles the maximum possible quantity of coils.
    #[test]
    fn test_parse_read_coils_response_max_quantity() {
        let max_quantity = MAX_COILS_PER_PDU as u16; // 2000 coils
        let expected_byte_count = ((max_quantity + 7) / 8) as u8; // 250 bytes

        let mut response_bytes_vec: Vec<u8, 253> = Vec::new(); // FC + Byte Count + 250 data bytes
        response_bytes_vec
            .push(FunctionCode::ReadCoils as u8)
            .unwrap();
        response_bytes_vec.push(expected_byte_count).unwrap();
        for i in 0..expected_byte_count as usize {
            response_bytes_vec.push(i as u8).unwrap(); // Fill with some dummy data
        }

        let pdu = Pdu::from_bytes(&response_bytes_vec).unwrap();
        let coils_data = ResponseParser::parse_read_coils_response(&pdu, max_quantity).unwrap();

        assert_eq!(coils_data.len(), expected_byte_count as usize);
        assert_eq!(coils_data.as_slice(), &response_bytes_vec.as_slice()[2..]); // Skip FC and Byte Count
    }

    /// Test case: `parse_read_coils_response` returns an error if the internal `Vec` capacity is exceeded.
    #[test]
    fn test_parse_read_coils_response_buffer_too_small_for_data() {
        // Craft a PDU that claims a byte count of 251, which would exceed MAX_COIL_BYTES (250)
        let expected_quantity = (MAX_COIL_BYTES * 8 + 1) as u16; // A quantity that would require more than MAX_COIL_BYTES
        let byte_count_in_pdu = ((expected_quantity + 7) / 8) as u8; // This would be 251 for 2001 coils

        let mut response_bytes_vec: Vec<u8, 253> = Vec::new();
        response_bytes_vec
            .push(FunctionCode::ReadCoils as u8)
            .unwrap();
        response_bytes_vec.push(byte_count_in_pdu).unwrap(); // Byte count = 251
        for _i in 0..byte_count_in_pdu as usize {
            response_bytes_vec.push(0x00).unwrap(); // Fill with dummy data
        }

        let pdu = Pdu::from_bytes(&response_bytes_vec).unwrap();
        let result = ResponseParser::parse_read_coils_response(&pdu, expected_quantity);
        assert_eq!(result.unwrap_err(), MbusError::BufferLenMissmatch);
    }

    // --- Write Single Coil Request Tests ---

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil ON.
    #[test]
    fn test_write_single_coil_request_on() {
        let address = 0x0005;
        let value = true;
        let pdu = ReqPduCompiler::write_single_coil_request(address, value).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleCoil);
        assert_eq!(pdu.data_len(), 4);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x05, 0xFF, 0x00]);
    }

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil OFF.
    #[test]
    fn test_write_single_coil_request_off() {
        let address = 0x0005;
        let value = false;
        let pdu = ReqPduCompiler::write_single_coil_request(address, value).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleCoil);
        assert_eq!(pdu.data_len(), 4);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x05, 0x00, 0x00]);
    }

    // --- Parse Write Single Coil Response Tests ---

    /// Test case: `parse_write_single_coil_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_single_coil_response_valid() {
        let response_bytes = [0x05, 0x00, 0x05, 0xFF, 0x00]; // FC, Address, Value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_single_coil_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_single_coil_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x05, 0xFF, 0x00]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_single_coil_response_address_mismatch() {
        let response_bytes = [0x05, 0x00, 0x06, 0xFF, 0x00]; // Address 0x0006, expected 0x0005
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::InvalidAddress);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for value mismatch.
    #[test]
    fn test_parse_write_single_coil_response_value_mismatch() {
        let response_bytes = [0x05, 0x00, 0x05, 0x00, 0x00]; // Value OFF, expected ON
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::InvalidValue);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_single_coil_response_invalid_len() {
        let response_bytes = [0x05, 0x00, 0x05, 0xFF]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::InvalidDataLen);
    }

    // --- Write Multiple Coils Request Tests ---

    /// Test case: `write_multiple_coils_request` creates a valid PDU for writing multiple coils.
    #[test]
    fn test_write_multiple_coils_request_valid() {
        let address = 0x0001;
        let quantity = 10; // 10 coils requires 2 bytes
        
        // Initialize Coils model and set specific bits
        // 0x55 = 0b0101_0101 (Bits 0, 2, 4, 6 are ON)
        // 0x01 = 0b0000_0001 (Bit 8 is ON)
        let mut coils = Coils::new(address, quantity);
        for i in (0..quantity).step_by(2) {
            coils.set_value(address + i, true).unwrap();
        }

        let pdu = ReqPduCompiler::write_multiple_coils_request(address, quantity, &coils).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleCoils);
        assert_eq!(pdu.data_len(), 5 + 2); // Addr (2) + Qty (2) + Byte Count (1) + Data (2) = 7
        assert_eq!(
            pdu.data().as_slice(),
            &[0x00, 0x01, 0x00, 0x0A, 0x02, 0x55, 0x01]
        );
    }

    /// Test case: `write_multiple_coils_request` returns an error for invalid quantity (too low).
    #[test]
    fn test_write_multiple_coils_request_invalid_quantity_low() {
        let coils = Coils::new(0x0001, 1);
        // Manually passing 0 as quantity to trigger validation in compiler
        let result = ReqPduCompiler::write_multiple_coils_request(0x0001, 0, &coils);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_coils_request` returns an error for invalid quantity (too high).
    #[test]
    fn test_write_multiple_coils_request_invalid_quantity_high() {
        let coils = Coils::new(0x0001, 1968);
        // Manually passing 1969 to exceed Modbus limit for FC 0x0F
        let result = ReqPduCompiler::write_multiple_coils_request(0x0001, 1969, &coils);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Parse Write Multiple Coils Response Tests ---

    /// Test case: `parse_write_multiple_coils_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_multiple_coils_response_valid() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0A]; // FC, Address, Quantity
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_multiple_coils_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x01, 0x00, 0x0A]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_address_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x02, 0x00, 0x0A]; // Address 0x0002, expected 0x0001
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::InvalidAddress);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for quantity mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_quantity_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0B]; // Quantity 11, expected 10
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::InvalidQuantity);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_multiple_coils_response_invalid_len() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ResponseParser::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::InvalidDataLen);
    }
}
