//! Modbus Coils Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to Coils (Function Codes 0x01, 0x05, and 0x0F).
//!
//! It includes functionality for:
//! - Reading multiple or single coils.
//! - Writing single or multiple coils.
//! - Packing and unpacking coil states into bit-fields within bytes.

use crate::data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu};
use crate::errors::MbusError;
use crate::function_codes::public::{FunctionCode, MAX_PDU_DATA_LEN};
use crate::transport::TransportType;
use heapless::Vec;

use core::usize;

/// Maximum number of coils that can be read/written in a single Modbus PDU (2000 coils).
const MAX_COILS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the coil states for 2000 coils (250 bytes).
pub const MAX_COIL_BYTES: usize = (MAX_COILS_PER_PDU + 7) / 8; // 250 bytes for 2000 coils

/// Represents the state of a block of coils read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Coils {
    /// The starting address of the first coil in this block.
    from_address: u16,
    /// The number of coils in this block.
    quantity: u16,
    /// The coil states packed into bytes, where each bit represents a coil (1 for ON, 0 for OFF).
    values: Vec<u8, MAX_COIL_BYTES>, // Each bit represents a coil state
}

/// Provides operations for reading and writing Modbus coils.
impl Coils {
    /// Creates a new `Coils` instance with the given starting address, quantity, and coil states.
    pub fn new(from_address: u16, quantity: u16, values: Vec<u8, MAX_COIL_BYTES>) -> Self {
        Self {
            from_address,
            quantity,
            values,
        }
    }

    /// Returns the starting address of the first coil in this block.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the number of coils in this block.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns a reference to the vector of bytes representing the coil states.
    pub fn values(&self) -> &Vec<u8, MAX_COIL_BYTES> {
        &self.values
    }

    /// Retrieves the boolean state of a specific coil by its address.
    pub fn value(&self, address: u16) -> Result<bool, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_mask = 1u8 << (bit_index % 8);

        Ok(self.values[byte_index] & bit_mask != 0)
    }
}

/// Service for handling Modbus coil operations, including creating request PDUs and parsing responses.
#[derive(Debug, Clone)]
pub struct CoilService;

/// Provides operations for reading and writing Modbus coils, as well as parsing responses for coil-related function codes.
impl CoilService {
    /// Creates a new `CoilService` instance.
    ///
    /// # Returns
    /// A new `CoilService` instance.
    pub fn new() -> Self {
        Self {}
    }

    /// Sends a Read Coils request to a Modbus server and registers the expected response.
    ///
    /// # Arguments
    /// * `txn_id` - The transaction ID for the request.
    /// * `unit_id` - The unit ID (slave address) of the Modbus server.
    /// * `address` - The starting address of the first coil to read (0-65535).
    /// * `quantity` - The number of coils to read (1-2000).
    /// * `single_read` - Whether this is a single coil read or multiple coils read.
    ///
    pub fn read_coils(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = CoilReqPdu::read_coils_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Single Coil request to a Modbus server and registers the expected response.
    ///
    /// # Arguments
    /// * `txn_id` - The transaction ID for the request.
    /// * `unit_id` - The unit ID (slave address) of the Modbus server.
    /// * `address` - The address of the coil to write (0-65535).
    /// * `value` - The state to write to the coil (`true` for ON, `false` for OFF).
    /// # Returns
    /// A `Result` containing the raw bytes of the Modbus ADU to be sent, or an `MbusError` if the request could not be created.
    pub fn write_single_coil(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: bool,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = CoilReqPdu::write_single_coil_request(address, value)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Multiple Coils request to a Modbus server and registers the expected response.
    pub fn write_multiple_coils(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[bool],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = CoilReqPdu::write_multiple_coils_request(address, quantity, values)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Handles a Read Coils response by invoking the appropriate application callback.
    pub fn handle_read_coil_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Coils, MbusError> {
        if function_code != FunctionCode::ReadCoils {
            return Err(MbusError::InvalidFunctionCode); // Mismatch in function code
        }
        let coil_response =
            match CoilReqPdu::handle_coil_response(pdu, expected_quantity, from_address) {
                Some(response) => response,
                None => {
                    // Parsing failed within CoilReqPdu.
                    return Err(MbusError::ParseError);
                }
            };

        Ok(coil_response)
    }

    /// Handles a Read Coils response for a single coil read by invoking the appropriate application callback.
    pub fn handle_write_single_coil_rsp(
        &mut self,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteSingleCoil {
            return Err(MbusError::InvalidFunctionCode);
        }
        if CoilReqPdu::parse_write_single_coil_response(pdu, address, value).is_ok() {
            Ok(())
        } else {
            Err(MbusError::ParseError)
        }
    }

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback.
    pub fn handle_write_multiple_coils_rsp(
        &mut self,
        function_code: FunctionCode,
        pdu: &crate::data_unit::common::Pdu,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteMultipleCoils {
            return Err(MbusError::InvalidFunctionCode);
        }
        if CoilReqPdu::parse_write_multiple_coils_response(pdu, address, quantity).is_ok() {
            Ok(())
        } else {
            Err(MbusError::ParseError)
        }
    }
}

/// Provides operations for reading and writing Modbus coils.
///
/// This struct is stateless and provides static methods to create request PDUs
/// and parse response PDUs for coil-related Modbus function codes.
pub struct CoilReqPdu {}

/// Provides operations for reading and writing Modbus coils, as well as parsing responses for coil-related function codes.
impl CoilReqPdu {
    /// Creates a new `CoilService` instance.
    ///
    /// # Returns
    /// A new `CoilService` instance.
    pub fn new() -> Self {
        Self {}
    }

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

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(
            FunctionCode::ReadCoils,
            data_vec,
            4, // 2 bytes for address, 2 bytes for quantity
        ))
    }

    /// Creates a Modbus PDU for a Read Coils (FC 0x01) request for a single coil.
    ///
    /// This is a convenience wrapper around `read_coils_request` with `quantity` set to 1.
    ///
    /// # Arguments
    /// * `address` - The address of the single coil to read (0-65535).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub fn read_single_coil_request(address: u16) -> Result<Pdu, MbusError> {
        Self::read_coils_request(address, 1)
    }

    /// Parses a Modbus PDU response for a Read Coils (FC 0x01) request for a single coil.
    ///
    /// This function interprets the PDU received from a Modbus server, extracting the
    /// boolean state of a single coil.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The address that was originally requested.
    ///
    /// # Returns
    /// A `Result` containing the boolean state of the coil, or an `MbusError` if
    /// the PDU is malformed or the data does not represent a single coil.
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
    ) -> Result<Vec<u8, MAX_COIL_BYTES>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadCoils {
            return Err(MbusError::ParseError);
        }

        let data_slice = pdu.data().as_slice();
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

        let coils = Vec::from_slice(&data_slice[1..]).map_err(|_| MbusError::BufferLenMissmatch)?;
        Ok(coils)
    }

    /// Parses a Modbus PDU response for a Read Coils (FC 0x01) request for a single coil.
    /// This is a convenience wrapper around `parse_read_coils_response` that extracts the state of a single coil.
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The address of the single coil that was originally requested.
    /// # Returns
    /// A `Result` containing the boolean state of the coil, or an `MbusError` if the PDU is malformed,
    /// contains an unexpected function code, the data length does not match the expected quantity,
    /// or the coil address is out of range.
    pub fn parse_read_single_coil_response(
        pdu: &Pdu,
        expected_address: u16,
    ) -> Result<bool, MbusError> {
        let coil_data_bytes = Self::parse_read_coils_response(pdu, 1)?;
        let coils = Coils::new(expected_address, 1, coil_data_bytes);
        coils.value(expected_address)
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
        macro_rules! push_be {
            ($vec:expr, $val:expr) => {
                $vec.extend_from_slice(&$val.to_be_bytes())
                    .map_err(|_| MbusError::BufferLenMissmatch)
            };
        }

        let mut data_bytes: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        push_be!(data_bytes, address)?;

        // Modbus protocol uses 0xFF00 for ON and 0x0000 for OFF
        let coil_value: u16 = if value { 0xFF00 } else { 0x0000 };
        push_be!(data_bytes, coil_value)?;

        Ok(Pdu::new(
            FunctionCode::WriteSingleCoil,
            data_bytes,
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

        let data_slice = pdu.data().as_slice();

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
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();

        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .push(byte_count)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Initialize bytes for coil data
        let num_coil_bytes = byte_count as usize;
        data_vec
            .resize(data_vec.len() + num_coil_bytes, 0)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for (i, &value) in values.iter().enumerate() {
            if value {
                let byte_index = 5 + (i / 8); // Offset by 5 (addr, qty, byte_count) in the PDU data
                let bit_index = i % 8;
                data_vec[byte_index] |= 1 << bit_index;
            }
        }

        Ok(Pdu::new(
            FunctionCode::WriteMultipleCoils,
            data_vec,
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

        let data_slice = pdu.data().as_slice();

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

    /// Handles a Read Coils response by invoking the appropriate application callback.
    /// This function parses the PDU received from a Modbus server in response to a Read Coils request,
    /// extracting the coil states and returning a `Coils` struct that can be used by the application layer.
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_quantity` - The quantity of coils that was originally requested.
    /// * `from_address` - The starting address of the coils that were requested.
    /// # Returns
    /// An `Option<Coils>` containing the parsed coil states if the response is valid, or
    /// `None` if the response is malformed or does not match the expected quantity.
    pub fn handle_coil_response(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Option<Coils> {
        let coil_response: Coils = if let Ok(coil_response) =
            CoilReqPdu::parse_read_coils_response(pdu, expected_quantity)
        {
            Coils::new(from_address, expected_quantity, coil_response)
        } else {
            return None; // If parsing fails, do not proceed with response handling
        };
        Some(coil_response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_codes::public::FunctionCode;

    // --- Read Coils Request Tests ---

    /// Test case: `read_coils_request` creates a valid PDU for reading coils.
    #[test]
    fn test_read_coils_request_valid() {
        let address = 0x0001;
        let quantity = 0x000A; // 10 coils
        let pdu = CoilReqPdu::read_coils_request(address, quantity).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::ReadCoils);
        assert_eq!(pdu.data_len(), 4);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x01, 0x00, 0x0A]);
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too low).
    #[test]
    fn test_read_coils_request_invalid_quantity_low() {
        let result = CoilReqPdu::read_coils_request(0x0001, 0);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `read_coils_request` returns an error for an invalid quantity (too high).
    #[test]
    fn test_read_coils_request_invalid_quantity_high() {
        let result = CoilReqPdu::read_coils_request(0x0001, 2001);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Parse Read Coils Response Tests ---

    /// Test case: `parse_read_coils_response` successfully parses a valid response for 8 coils.
    #[test]
    fn test_parse_read_coils_response_valid_8_coils() {
        // Response for reading 8 coils, values: 10110011 (0xB3)
        // PDU: FC (0x01), Byte Count (0x01), Coil Data (0xB3)
        let response_bytes = [0x01, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let coils_data = CoilReqPdu::parse_read_coils_response(&pdu, 8).unwrap();

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
        let coils_data = CoilReqPdu::parse_read_coils_response(&pdu, 10).unwrap();

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
        let coils_data = CoilReqPdu::parse_read_coils_response(&pdu, 3).unwrap();

        assert_eq!(coils_data.as_slice(), &[0x05]);
        assert_eq!(coils_data.len(), 1);
    }

    /// Test case: `parse_read_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_read_coils_response_wrong_fc() {
        // PDU with FC 0x03 (Read Holding Registers) instead of 0x01 (Read Coils)
        let response_bytes = [0x03, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_read_coils_response` returns an error for an empty data slice (PDU only contains FC).
    #[test]
    fn test_parse_read_coils_response_empty_data() {
        // PDU: FC (0x01) only, no byte count or coil data
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = CoilReqPdu::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `parse_read_coils_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_coils_response_byte_count_mismatch() {
        // PDU: FC (0x01), Byte Count (0x01), but provides two data bytes (0xB3, 0x00)
        let response_bytes = [0x01, 0x01, 0xB3, 0x00];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_read_coils_response(&pdu, 8);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `parse_read_coils_response` returns an error for expected quantity mismatch with actual byte count.
    #[test]
    fn test_parse_read_coils_response_expected_quantity_mismatch() {
        // PDU: FC (0x01), Byte Count (0x01), Coil Data (0xB3) -> implies 8 coils
        // But `expected_quantity` is 16, which would require 2 bytes of coil data.
        let response_bytes = [0x01, 0x01, 0xB3];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_read_coils_response(&pdu, 16);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
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
        let coils_data = CoilReqPdu::parse_read_coils_response(&pdu, max_quantity).unwrap();

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
        let result = CoilReqPdu::parse_read_coils_response(&pdu, expected_quantity);
        assert_eq!(result.unwrap_err(), MbusError::BufferLenMissmatch);
    }

    // --- Write Single Coil Request Tests ---

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil ON.
    #[test]
    fn test_write_single_coil_request_on() {
        let address = 0x0005;
        let value = true;
        let pdu = CoilReqPdu::write_single_coil_request(address, value).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleCoil);
        assert_eq!(pdu.data_len(), 4);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x05, 0xFF, 0x00]);
    }

    /// Test case: `write_single_coil_request` creates a valid PDU for writing a single coil OFF.
    #[test]
    fn test_write_single_coil_request_off() {
        let address = 0x0005;
        let value = false;
        let pdu = CoilReqPdu::write_single_coil_request(address, value).unwrap();

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
        let result = CoilReqPdu::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_single_coil_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_single_coil_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x05, 0xFF, 0x00]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_single_coil_response_address_mismatch() {
        let response_bytes = [0x05, 0x00, 0x06, 0xFF, 0x00]; // Address 0x0006, expected 0x0005
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for value mismatch.
    #[test]
    fn test_parse_write_single_coil_response_value_mismatch() {
        let response_bytes = [0x05, 0x00, 0x05, 0x00, 0x00]; // Value OFF, expected ON
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_single_coil_response(&pdu, 0x0005, true);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_single_coil_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_single_coil_response_invalid_len() {
        let response_bytes = [0x05, 0x00, 0x05, 0xFF]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_single_coil_response(&pdu, 0x0005, true);
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
        let pdu = CoilReqPdu::write_multiple_coils_request(address, quantity, &values).unwrap();

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
        let values = [true];
        let result = CoilReqPdu::write_multiple_coils_request(0x0001, 0, &values);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_coils_request` returns an error for invalid quantity (too high).
    #[test]
    fn test_write_multiple_coils_request_invalid_quantity_high() {
        let values = [true; 1969]; // Too many
        let result = CoilReqPdu::write_multiple_coils_request(0x0001, 1969, &values);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_coils_request` returns an error for quantity-values mismatch.
    #[test]
    fn test_write_multiple_coils_request_quantity_values_mismatch() {
        let values = [true, false];
        let result = CoilReqPdu::write_multiple_coils_request(0x0001, 3, &values); // Quantity 3, but only 2 values
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    // --- Parse Write Multiple Coils Response Tests ---

    /// Test case: `parse_write_multiple_coils_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_multiple_coils_response_valid() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0A]; // FC, Address, Quantity
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert!(result.is_ok());
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for a wrong function code.
    #[test]
    fn test_parse_write_multiple_coils_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x01, 0x00, 0x0A]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_address_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x02, 0x00, 0x0A]; // Address 0x0002, expected 0x0001
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for quantity mismatch.
    #[test]
    fn test_parse_write_multiple_coils_response_quantity_mismatch() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00, 0x0B]; // Quantity 11, expected 10
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::ParseError);
    }

    /// Test case: `parse_write_multiple_coils_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_write_multiple_coils_response_invalid_len() {
        let response_bytes = [0x0F, 0x00, 0x01, 0x00]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let result = CoilReqPdu::parse_write_multiple_coils_response(&pdu, 0x0001, 10);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }
}
