use crate::data_unit::common::{AdditionalAddress, Data, MbapHeader, ModbusMessage, Pdu};
use crate::errors::MbusError;
use crate::function_codes::public::FunctionCode;
use crate::transport::{TransportType};
use heapless::Vec;

pub struct Coils {
}

impl Coils {
    pub fn new() -> Self {
        Self {}
    }

    pub fn read_multiple(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<ModbusMessage, MbusError> {
        let pdu = ModbusCoilService::read_coils_request(address, quantity);
        let adu = match transport_type {
            TransportType::StdTcp | TransportType::CustomTcp => {
                // Construct Modbus TCP ADU
                let pdu = pdu?;
                let mbap_header = MbapHeader::new(txn_id, pdu.data_len() as u16 + 1, unit_id); // Transaction ID, Protocol ID, Length, Unit ID
                ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu)
            }
            TransportType::StdSerial | TransportType::CustomSerial => {
                // For Modbus RTU/ASCII, the PDU is the ADU (with framing handled by transport)
                todo!()
            }
        };

        

        Ok(adu)
    }
}

/// Provides operations for reading and writing Modbus coils.
///
/// This struct is stateless and provides static methods to create request PDUs
/// and parse response PDUs for coil-related Modbus function codes.
pub struct ModbusCoilService {}

impl ModbusCoilService {
    /// Creates a Modbus PDU for a Read Coils (FC 0x01) request.
    ///
    /// This function constructs the PDU required to read the ON/OFF status of
    /// a contiguous block of coils from a Modbus server.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first coil to read (0-65535).
    /// * `quantity` - The number of coils to read (1-2000).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity is out of the valid Modbus range (1 to 2000).
    pub fn read_coils_request(address: u16, quantity: u16) -> Result<Pdu, MbusError> {
        if !(1..=2000).contains(&quantity) {
            return Err(MbusError::InvalidPduLength); // Quantity out of range
        }

        let mut data_bytes = [0u8; 252];
        data_bytes[0..2].copy_from_slice(&address.to_be_bytes());
        data_bytes[2..4].copy_from_slice(&quantity.to_be_bytes());

        Ok(Pdu::new(
            FunctionCode::ReadCoils,
            Data::Bytes(data_bytes),
            4, // 2 bytes for address, 2 bytes for quantity
        ))
    }

    /// Parses a Modbus PDU response for a Read Coils (FC 0x01) request.
    ///
    /// This function interprets the PDU received from a Modbus server in response
    /// to a Read Coils request, extracting the coil states.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_quantity` - The quantity of coils that was originally requested.
    ///
    /// # Returns
    /// A `Result` containing a `heapless::Vec<bool, 2000>` representing the coil states,
    /// or an `MbusError` if the PDU is malformed, contains an unexpected function code,
    /// or the data length does not match the expected quantity.
    pub fn parse_read_coils_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<bool, 2000>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadCoils {
            return Err(MbusError::ParseError);
        }

        let data_slice = match pdu.data() {
            Data::Bytes(bytes) => &bytes[..pdu.data_len() as usize],
            _ => return Err(MbusError::ParseError), // Unexpected data type
        };

        if data_slice.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let byte_count = data_slice[0] as usize;
        // The PDU data should be: [byte_count, data_byte_1, ..., data_byte_N]
        // So, total length of data_slice should be 1 (for byte_count) + byte_count
        if byte_count + 1 != data_slice.len() {
            return Err(MbusError::InvalidPduLength);
        }

        // Calculate expected byte count: ceil(expected_quantity / 8)
        let expected_byte_count = ((expected_quantity + 7) / 8) as usize;
        if byte_count != expected_byte_count {
            return Err(MbusError::ParseError); // Mismatch in expected byte count
        }

        let mut coils = Vec::new();
        for i in 0..expected_quantity {
            let byte_index = (i / 8) as usize;
            let bit_index = (i % 8) as u8;
            // Coil data starts from data_slice[1]
            let is_set = (data_slice[1 + byte_index] >> bit_index) & 0x01 == 0x01;
            coils.push(is_set).map_err(|_| MbusError::BufferTooSmall)?; // Should not happen with correct capacity
        }
        Ok(coils)
    }

    /// Creates a Modbus PDU for a Write Single Coil (FC 0x05) request.
    ///
    /// This function constructs the PDU required to force a single coil to
    /// either ON (0xFF00) or OFF (0x0000) state.
    ///
    /// # Arguments
    /// * `address` - The address of the coil to write (0-65535).
    /// * `value` - The state to write to the coil (`true` for ON, `false` for OFF).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub fn write_single_coil_request(address: u16, value: bool) -> Result<Pdu, MbusError> {
        let mut data_bytes = [0u8; 252];
        data_bytes[0..2].copy_from_slice(&address.to_be_bytes());
        data_bytes[2..4]
            .copy_from_slice(&(if value { 0xFF00u16 } else { 0x0000u16 }).to_be_bytes());

        Ok(Pdu::new(
            FunctionCode::WriteSingleCoil,
            Data::Bytes(data_bytes),
            4, // 2 bytes for address, 2 bytes for value
        ))
    }

    /// Parses a Modbus PDU response for a Write Single Coil (FC 0x05) request.
    ///
    /// This function validates the response from a Modbus server for a Write Single Coil
    /// operation, ensuring the function code, address, and value match the request.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The address that was written in the request.
    /// * `expected_value` - The value that was written in the request.
    ///
    /// # Returns
    /// `Ok(())` if the response is valid and matches the request, or an `MbusError` otherwise.
    pub fn parse_write_single_coil_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_value: bool,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteSingleCoil {
            return Err(MbusError::ParseError);
        }

        let data_slice = match pdu.data() {
            Data::Bytes(bytes) => &bytes[..pdu.data_len() as usize],
            _ => return Err(MbusError::ParseError),
        };

        if data_slice.len() != 4 {
            // Address (2 bytes) + Value (2 bytes)
            return Err(MbusError::InvalidPduLength);
        }

        let response_address = u16::from_be_bytes([data_slice[0], data_slice[1]]);
        let response_value = u16::from_be_bytes([data_slice[2], data_slice[3]]);

        if response_address != expected_address {
            return Err(MbusError::ParseError); // Address mismatch
        }

        let expected_response_value = if expected_value { 0xFF00 } else { 0x0000 };
        if response_value != expected_response_value {
            return Err(MbusError::ParseError); // Value mismatch
        }

        Ok(())
    }

    /// Creates a Modbus PDU for a Write Multiple Coils (FC 0x0F) request.
    ///
    /// This function constructs the PDU required to force a contiguous block of
    /// coils to specific ON/OFF states.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first coil to write (0-65535).
    /// * `quantity` - The number of coils to write (1-1968).
    /// * `values` - A slice of booleans representing the coil states to write.
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity or the length of `values` is invalid.
    pub fn write_multiple_coils_request(
        address: u16,
        quantity: u16,
        values: &[bool],
    ) -> Result<Pdu, MbusError> {
        // Max quantity for Write Multiple Coils is 1968.
        // PDU data: Address (2 bytes) + Quantity (2 bytes) + Byte Count (1 byte) + Coil Status (N bytes)
        // Max PDU data length is 252.
        // 2 + 2 + 1 + ceil(1968/8) = 5 + 246 = 251 bytes. This fits.
        if !(1..=1968).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        if values.len() as u16 != quantity {
            return Err(MbusError::InvalidPduLength); // Mismatch between quantity and values length
        }

        let byte_count = ((quantity + 7) / 8) as u8;
        let mut data_bytes = [0u8; 252];

        data_bytes[0..2].copy_from_slice(&address.to_be_bytes());
        data_bytes[2..4].copy_from_slice(&quantity.to_be_bytes());
        data_bytes[4] = byte_count;

        for (i, &value) in values.iter().enumerate() {
            if value {
                let byte_index = 5 + (i / 8); // Offset by 5 (addr, qty, byte_count)
                let bit_index = i % 8;
                data_bytes[byte_index] |= 1 << bit_index;
            }
        }

        Ok(Pdu::new(
            FunctionCode::WriteMultipleCoils,
            Data::Bytes(data_bytes),
            5 + byte_count as u8, // 2 bytes addr + 2 bytes qty + 1 byte byte_count + N bytes coil data
        ))
    }

    /// Parses a Modbus PDU response for a Write Multiple Coils (FC 0x0F) request.
    ///
    /// This function validates the response from a Modbus server for a Write Multiple Coils
    /// operation, ensuring the function code, starting address, and quantity match the request.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The starting address that was written in the request.
    /// * `expected_quantity` - The quantity of coils that was written in the request.
    ///
    /// # Returns
    /// `Ok(())` if the response is valid and matches the request, or an `MbusError` otherwise.
    pub fn parse_write_multiple_coils_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteMultipleCoils {
            return Err(MbusError::ParseError);
        }

        let data_slice = match pdu.data() {
            Data::Bytes(bytes) => &bytes[..pdu.data_len() as usize],
            _ => return Err(MbusError::ParseError),
        };

        if data_slice.len() != 4 {
            // Address (2 bytes) + Quantity (2 bytes)
            return Err(MbusError::InvalidPduLength);
        }

        let response_address = u16::from_be_bytes([data_slice[0], data_slice[1]]);
        let response_quantity = u16::from_be_bytes([data_slice[2], data_slice[3]]);

        if response_address != expected_address || response_quantity != expected_quantity {
            return Err(MbusError::ParseError); // Mismatch in address or quantity
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{function_codes::public::FunctionCode};

    // --- Read Coils Request Tests ---

    /// Test case: `read_coils_request` creates a valid PDU for reading coils.
    #[test]
    fn test_read_coils_request_valid() {
        let address = 0x0001;
        let quantity = 0x000A; // 10 coils
        let pdu = ModbusCoilService::read_coils_request(address, quantity).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::ReadCoils);
        assert_eq!(pdu.data_len(), 4);
        if let Data::Bytes(data) = pdu.data() {
            assert_eq!(&data[0..4], &[0x00, 0x01, 0x00, 0x0A]);
        } else {
            panic!("Expected Data::Bytes");
        }
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too low).
    #[test]
    fn test_read_coils_request_invalid_quantity_low() {
        let result = ModbusCoilService::read_coils_request(0x0001, 0);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too high).
    #[test]
    fn test_read_coils_request_invalid_quantity_high() {
        let result = ModbusCoilService::read_coils_request(0x0001, 2001);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Parse Read Coils Response Tests ---

    /// Test case: `parse_read_coils_response` successfully parses a valid response.
    #[test]
    fn test_parse_read_coils_response_valid() {
        // Response for reading 8 coils, values: 10110011 (0xB3)
        let response_bytes = [0x01, 0x01, 0xB3]; // FC, Byte Count, Coil Data
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils = ModbusCoilService::parse_read_coils_response(&pdu, 8).unwrap();

        assert_eq!(coils.len(), 8);
        assert_eq!(coils[0], true); // 1
        assert_eq!(coils[1], true); // 1
        assert_eq!(coils[2], false); // 0
        assert_eq!(coils[3], false); // 0
        assert_eq!(coils[4], true); // 1
        assert_eq!(coils[5], true); // 1
        assert_eq!(coils[6], false); // 0
        assert_eq!(coils[7], true); // 1
    }

    /// Test case: `parse_read_coils_response` parses a response with multiple bytes of coil data.
    #[test]
    fn test_parse_read_coils_response_multiple_bytes() {
        // Response for reading 10 coils, values: 10110011 (0xB3), 00000011 (0x03)
        let response_bytes = [0x01, 0x02, 0xB3, 0x03]; // FC, Byte Count, Coil Data
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils = ModbusCoilService::parse_read_coils_response(&pdu, 10).unwrap();

        assert_eq!(coils.len(), 10);
        assert_eq!(coils[0], true);
        assert_eq!(coils[1], true);
        assert_eq!(coils[2], false);
        assert_eq!(coils[3], false);
        assert_eq!(coils[4], true);
        assert_eq!(coils[5], true);
        assert_eq!(coils[6], false);
        assert_eq!(coils[7], true);
        assert_eq!(coils[8], true);
        assert_eq!(coils[9], true);
    }

    /// Test case: `parse_read_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_read_coils_response_wrong_fc() {
        let response_bytes = [0x03, 0x01, 0xB3]; // Wrong FC (Read Holding Registers)
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_read_coils_response` returns an error for an empty data slice.
    #[test]
    fn test_parse_read_coils_response_empty_data() {
        let response_bytes = [0x01]; // Only FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `parse_read_coils_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_coils_response_byte_count_mismatch() {
        // Response indicates 1 byte of data, but provides 2
        let response_bytes = [0x01, 0x01, 0xB3, 0x00]; // FC, Byte Count=1, Data=0xB3, 0x00
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `parse_read_coils_response` returns an error for expected quantity mismatch with actual byte count.
    #[test]
    fn test_parse_read_coils_response_expected_quantity_mismatch() {
        // Response for 8 coils (1 byte data), but expected 16 coils (2 bytes data)
        let response_bytes = [0x01, 0x01, 0xB3]; // FC, Byte Count=1, Data=0xB3
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_read_coils_response(&pdu, 16); // Expecting 16 coils, which needs 2 bytes
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    // --- Write Single Coil Request Tests ---

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil ON.
    #[test]
    fn test_write_single_coil_request_on() {
        let address = 0x0005;
        let value = true;
        let pdu = ModbusCoilService::write_single_coil_request(address, value).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleCoil);
        assert_eq!(pdu.data_len(), 4);
        if let Data::Bytes(data) = pdu.data() {
            assert_eq!(&data[0..4], &[0x00, 0x05, 0xFF, 0x00]);
        } else {
            panic!("Expected Data::Bytes");
        }
    }

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil OFF.
    #[test]
    fn test_write_single_coil_request_off() {
        let address = 0x0005;
        let value = false;
        let pdu = ModbusCoilService::write_single_coil_request(address, value).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleCoil);
        assert_eq!(pdu.data_len(), 4);
        if let Data::Bytes(data) = pdu.data() {
            assert_eq!(&data[0..4], &[0x00, 0x05, 0x00, 0x00]);
        } else {
            panic!("Expected Data::Bytes");
        }
    }

    // --- Parse Write Single Coil Response Tests ---

    /// Test case: `parse_write_single_coil_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_single_coil_response_valid() {
        let response_bytes = [0x05, 0x00, 0x05, 0xFF, 0x00]; // FC, Address, Value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_single_coil_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_single_coil_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x05, 0xFF, 0x00]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_single_coil_response_address_mismatch() {
        let response_bytes = [0x05, 0x00, 0x06, 0xFF, 0x00]; // Address 0x0006, expected 0x0005
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for value mismatch.
    #[test]
    fn test_parse_write_single_coil_response_value_mismatch() {
        let response_bytes = [0x05, 0x00, 0x05, 0x00, 0x00]; // Value OFF, expected ON
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_single_coil_response_invalid_len() {
        let response_bytes = [0x05, 0x00, 0x05, 0xFF]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Write Multiple Coils Request Tests ---

    /// Test case: `write_multiple_coils_request` creates a valid PDU for writing multiple coils.
    #[test]
    fn test_write_multiple_coils_request_valid() {
        let address = 0x0001;
        let quantity = 10;
        let values = [
            true, false, true, false, true, false, true, false, true, false,
        ]; // 0xAA, 0x02
        let pdu =
            ModbusCoilService::write_multiple_coils_request(address, quantity, &values).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleCoils);
        assert_eq!(pdu.data_len(), 5 + 2); // Addr (2) + Qty (2) + Byte Count (1) + Data (2) = 7
        if let Data::Bytes(data) = pdu.data() {
            assert_eq!(&data[0..7], &[0x00, 0x01, 0x00, 0x0A, 0x02, 0x55, 0x01]);
        } else {
            panic!("Expected Data::Bytes");
        }
    }

    /// Test case: `write_multiple_coils_request` returns an error for invalid quantity (too low).
    #[test]
    fn test_write_multiple_coils_request_invalid_quantity_low() {
        let values = [true];
        let result = ModbusCoilService::write_multiple_coils_request(0x0001, 0, &values);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_coils_request` returns an error for invalid quantity (too high).
    #[test]
    fn test_write_multiple_coils_request_invalid_quantity_high() {
        let values = [true; 1969]; // Too many
        let result = ModbusCoilService::write_multiple_coils_request(0x0001, 1969, &values);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_coils_request` returns an error for quantity-values mismatch.
    #[test]
    fn test_write_multiple_coils_request_quantity_values_mismatch() {
        let values = [true, false];
        let result = ModbusCoilService::write_multiple_coils_request(0x0001, 3, &values); // Quantity 3, but only 2 values
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Parse Write Multiple Coils Response Tests ---

    /// Test case: `parse_write_multiple_coils_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_multiple_coils_response_valid() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0A]; // FC, Address, Quantity
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_multiple_coils_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x01, 0x00, 0x0A]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_address_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x02, 0x00, 0x0A]; // Address 0x0002, expected 0x0001
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for quantity mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_quantity_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0B]; // Quantity 11, expected 10
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_multiple_coils_response_invalid_len() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = ModbusCoilService::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Tests for Coils::read_multiple ---

    /// Test case: `read_multiple` creates a valid ModbusMessage for TCP transport.
    #[test]
    fn test_read_multiple_tcp_valid() {
        let coils_service = Coils::new();
        let txn_id = 0x1234;
        let unit_id = 0x01;
        let address = 0x0001;
        let quantity = 0x000A; // 10 coils

        let modbus_message = coils_service
            .read_multiple(txn_id, unit_id, address, quantity, TransportType::StdTcp)
            .expect("Should successfully create ModbusMessage");

        // Verify AdditionalAddress (MbapHeader)
        if let AdditionalAddress::MbapHeader(header) = modbus_message.additional_address {
            assert_eq!(header.transaction_id, txn_id);
            assert_eq!(header.protocol_id, 0); // Always 0 for Modbus
            assert_eq!(header.length, 5); // PDU data_len (4) + 1 (unit_id)
            assert_eq!(header.unit_id, unit_id);
        } else {
            panic!("Expected MbapHeader for TCP transport");
        }

        // Verify PDU
        assert_eq!(modbus_message.pdu.function_code(), FunctionCode::ReadCoils);
        assert_eq!(modbus_message.pdu.data_len(), 4);
        if let Data::Bytes(data) = modbus_message.pdu.data() {
            assert_eq!(&data[0..4], &[0x00, 0x01, 0x00, 0x0A]);
        } else {
            panic!("Expected Data::Bytes in PDU");
        }
    }

    /// Test case: `read_multiple` handles invalid quantity from `read_coils_request`.
    #[test]
    fn test_read_multiple_invalid_quantity() {
        let coils_service = Coils::new();
        let txn_id = 0x1234;
        let unit_id = 0x01;
        let address = 0x0001;
        let invalid_quantity = 0; // Quantity out of range (1-2000)

        let result = coils_service.read_multiple(
            txn_id,
            unit_id,
            address,
            invalid_quantity,
            TransportType::StdTcp,
        );

        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }
}
