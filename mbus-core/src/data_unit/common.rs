use crate::errors::MbusError;
use crate::function_codes::public::{
    DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode,
};
use heapless::Vec;

pub const MAX_PDU_DATA_LEN: usize = 252; // Maximum data length for a PDU (excluding function code)

/// Represents sub-function codes used by specific Modbus function codes.
///
/// This union allows treating the sub-code as either a 16-bit Diagnostic sub-function
/// (FC 0x08) or an 8-bit Encapsulated Interface type (FC 0x2B).
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum SubCode {
    /// Sub-function code for Diagnostics (Function Code 0x08).
    DiagnosticSubFunction(DiagnosticSubFunction),
    /// MEI type for Encapsulated Interface Transport (Function Code 0x2B).
    EncapsulatedInterfaceType(EncapsulatedInterfaceType),
}

/// A structure combining a sub-function code with its associated payload.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct SubCodeBytes {
    /// The specific sub-function or MEI type.
    pub sub_code: SubCode,
    /// The remaining payload bytes for the sub-function.
    pub bytes: [u8; MAX_PDU_DATA_LEN - 2], // Subtract 2 bytes for the sub-code itself
}

/// The data payload of a Modbus PDU.
///
/// This union provides different views of the PDU data depending on whether
/// the function code uses sub-function codes or raw byte arrays.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
#[repr(C)]
pub enum Data {
    /// Raw byte access to the data field (Max 252 bytes).
    Bytes([u8; MAX_PDU_DATA_LEN]),
    /// Structured access for functions using sub-codes (e.g., FC 0x08, 0x2B).
    SubCodeBytes(SubCodeBytes),
}

/// Modbus Protocol Data Unit (PDU).
///
/// The PDU is the core of the Modbus message, consisting of a function code
/// and the associated data payload.
#[derive(Debug, Clone)]
pub struct Pdu {
    /// The Modbus function code identifying the operation.
    function_code: FunctionCode,
    /// The data payload associated with the function code.
    data: heapless::Vec<u8, MAX_PDU_DATA_LEN>,
    /// The actual length of the data payload (excluding the function code).
    data_len: u8,
}

/// Modbus TCP Application Data Unit (ADU) Header (MBAP).
#[derive(Debug, Clone, Copy)]
pub struct MbapHeader {
    /// Identification of a Modbus Request/Response transaction.
    pub transaction_id: u16,
    /// Protocol Identifier (0 = Modbus protocol).
    pub protocol_id: u16,
    /// Number of remaining bytes in the message (Unit ID + PDU).
    pub length: u16,
    /// Identification of a remote server on a non-TCP/IP network.
    pub unit_id: u8,
}

impl MbapHeader {
    /// Creates a new `MbapHeader` instance.
    ///
    /// # Arguments
    /// * `transaction_id` - The transaction ID for the Modbus message.
    /// * `protocol_id` - The protocol identifier (should be 0 for Modbus).
    /// * `length` - The length of the remaining message (Unit ID + PDU).
    /// * `unit_id` - The unit identifier for the target slave device.
    ///
    /// # Returns
    /// A new `MbapHeader` instance.
    pub fn new(transaction_id: u16, length: u16, unit_id: u8) -> Self {
        Self {
            transaction_id,
            protocol_id: 0, /* Must be 0 for Modbus */
            length,
            unit_id,
        }
    }
}

/// Additional address field for Modbus RTU/TCP messages.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum AdditionalAddress {
    /// The additional address field used in certain Modbus function codes.
    MbapHeader(MbapHeader),
    SlaveAddress(u8),
}

/// Represents a complete Modbus message, including the additional address, PDU and Error check.
#[derive(Debug, Clone)]
pub struct ModbusMessage {
    /// The MBAP header for Modbus TCP messages.
    pub additional_address: AdditionalAddress,
    /// The Protocol Data Unit (PDU) containing the function code and data.
    pub pdu: Pdu,
    // Error check (CRC for RTU, LRC for ASCII) would be handled separately based on the transport layer.
}

impl ModbusMessage {
    /// Creates a new `ModbusMessage` instance.
    ///
    /// # Arguments
    /// * `additional_address` - The additional address information (MBAP header or slave address).
    /// * `pdu` - The Protocol Data Unit containing the function code and data.
    ///
    /// # Returns
    /// A new `ModbusMessage` instance.
    pub fn new(additional_address: AdditionalAddress, pdu: Pdu) -> Self {
        Self {
            additional_address,
            pdu,
        }
    }

    /// Accessor for the additional address.
    pub fn additional_address(&self) -> &AdditionalAddress {
        &self.additional_address
    }

    /// Accessor for the function code from the PDU.
    pub fn function_code(&self) -> FunctionCode {
        self.pdu.function_code()
    }

    /// Accessor for the data payload from the PDU.
    pub fn data(&self) -> &heapless::Vec<u8, MAX_PDU_DATA_LEN> {
        self.pdu.data()
    }

    /// Accessor for the actual length of the data payload.
    pub fn data_len(&self) -> u8 {
        self.pdu.data_len()
    }

    /// Converts the `ModbusMessage` into its byte representation.
    ///
    /// This method serializes the additional address (MBAP header or slave address)
    /// followed by the PDU.
    ///
    /// # Returns
    /// `Ok(Vec<u8, 260>)` containing the ADU bytes, or an `MbusError` if
    /// the message cannot be serialized.
    pub fn to_bytes(&self) -> Result<Vec<u8, 260>, MbusError> {
        let mut adu_bytes = Vec::new();

        match &self.additional_address {
            AdditionalAddress::MbapHeader(header) => {
                // MBAP Header: TID (2), PID (2), Length (2), Unit ID (1)
                adu_bytes
                    .extend_from_slice(&header.transaction_id.to_be_bytes())
                    .map_err(|_| MbusError::Unexpected)?;
                adu_bytes
                    .extend_from_slice(&header.protocol_id.to_be_bytes())
                    .map_err(|_| MbusError::Unexpected)?;
                adu_bytes
                    .extend_from_slice(&header.length.to_be_bytes())
                    .map_err(|_| MbusError::Unexpected)?;
                adu_bytes
                    .push(header.unit_id)
                    .map_err(|_| MbusError::Unexpected)?;
            }
            AdditionalAddress::SlaveAddress(address) => {
                adu_bytes
                    .push(*address)
                    .map_err(|_| MbusError::Unexpected)?;
            }
        }

        let pdu_bytes = self.pdu.to_bytes()?;
        adu_bytes
            .extend_from_slice(&pdu_bytes)
            .map_err(|_| MbusError::Unexpected)?;

        Ok(adu_bytes)
    }

}

impl Pdu {
    ///
    /// Creates a new `Pdu` instance.
    ///
    /// # Arguments
    /// * `function_code` - The Modbus function code.
    /// * `data` - The data payload (either raw bytes or structured sub-codes).
    /// * `data_len` - The actual length of the data payload in bytes.
    ///
    /// # Returns
    /// A new `Pdu` instance.
    pub fn new(function_code: FunctionCode, data: heapless::Vec<u8, MAX_PDU_DATA_LEN>, data_len: u8) -> Self {
        Self {
            function_code,
            data: data, // Ensure the heapless::Vec is moved here
            data_len,
        }
    }

    /// Accessor for the function code.
    pub fn function_code(&self) -> FunctionCode {
        self.function_code
    }

    /// Accessor for the data payload.
    pub fn data(&self) -> &Vec<u8, MAX_PDU_DATA_LEN> {
        &self.data
    }

    /// Accessor for the actual length of the data payload.
    pub fn data_len(&self) -> u8 {
        self.data_len
    }

    /// Converts the PDU into its byte representation.
    ///
    /// This method serializes the function code and its associated data payload.
    /// It uses an `unsafe` block to access the `Data` union, assuming that
    /// `self.data.bytes` contains the full data payload and `self.data_len`
    /// accurately reflects its length.
    ///
    /// # Returns
    /// `Ok(Vec<u8, 253>)` containing the PDU bytes, or an `MbusError` if
    /// the PDU cannot be serialized (e.g., due to buffer overflow).
    pub fn to_bytes(&self) -> Result<Vec<u8, 253>, MbusError> {
        let mut pdu_bytes = Vec::new(); // Capacity is 253 (1 byte FC + 252 bytes data)
        pdu_bytes.push(self.function_code as u8).map_err(|_| MbusError::Unexpected)?; // Function code (1 byte)

        pdu_bytes
            .extend_from_slice(&self.data.as_slice()[..self.data_len as usize])
            .map_err(|_| MbusError::BufferLenMissmatch)?; // Data bytes (variable length)

        Ok(pdu_bytes)
    }

    /// Creates a PDU from its byte representation.
    ///
    /// This method parses the function code and data payload from the given byte slice.
    ///
    /// # Arguments
    /// * `bytes` - A byte slice containing the PDU (Function Code + Data).
    ///
    /// # Returns
    /// `Ok(Pdu)` if the bytes represent a valid PDU, or an `MbusError` otherwise.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MbusError> {
        if bytes.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let function_code = FunctionCode::try_from(bytes[0])?;

        let data_slice = &bytes[1..];
        let data_len = data_slice.len();

        if data_len > MAX_PDU_DATA_LEN {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data = heapless::Vec::new();
        data.extend_from_slice(data_slice)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu {
            function_code,
            data,
            data_len: data_len as u8,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_codes::public::FunctionCode;
    use heapless::Vec;

    // --- Tests for Pdu::from_bytes ---

    /// Test case: `Pdu::from_bytes` with a valid PDU that has no data bytes.
    ///
    /// This covers function codes like `ReportServerId` (0x11) which consist only of the function code.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.13 (Report Server ID).
    #[test]
    fn test_pdu_from_bytes_valid_no_data() {
        // Example: Report Server ID (0x11) request has no data bytes.
        let bytes = [0x11];
        let pdu = Pdu::from_bytes(&bytes).expect("Should successfully parse PDU with no data");

        assert_eq!(pdu.function_code, FunctionCode::ReportServerId);
        assert_eq!(pdu.data_len, 0);
        assert!(pdu.data.is_empty());
        assert_eq!(pdu.data.len(), 0);
    }

    /// Test case: `Pdu::from_bytes` with a valid `Read Coils` request PDU.
    ///
    /// This tests parsing a function code followed by address and quantity bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.1 (Read Coils).
    #[test]
    fn test_pdu_from_bytes_valid_read_coils_request() {
        // Read Coils (0x01) request: FC (1 byte) + Starting Address (2 bytes) + Quantity of Coils (2 bytes)
        // Example: Read 10 coils starting at address 0x0000
        let bytes = [0x01, 0x00, 0x00, 0x00, 0x0A]; // FC, Addr_Hi, Addr_Lo, Qty_Hi, Qty_Lo
        let pdu = Pdu::from_bytes(&bytes).expect("Should successfully parse Read Coils request");

        assert_eq!(pdu.function_code, FunctionCode::ReadCoils);
        assert_eq!(pdu.data_len, 4);
        assert_eq!(pdu.data.as_slice(), &[0x00, 0x00, 0x00, 0x0A]);
    }

    /// Test case: `Pdu::from_bytes` with a valid `Read Holding Registers` response PDU.
    ///
    /// This tests parsing a function code, a byte count, and then the actual data bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.3 (Read Holding Registers).
    #[test]
    fn test_pdu_from_bytes_valid_read_holding_registers_response() {
        // Read Holding Registers (0x03) response: FC (1 byte) + Byte Count (1 byte) + Data (N bytes)
        // Example: Response with 2 registers (4 bytes of data)
        let bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78]; // FC, Byte Count, Reg1_Hi, Reg1_Lo, Reg2_Hi, Reg2_Lo
        let pdu = Pdu::from_bytes(&bytes)
            .expect("Should successfully parse Read Holding Registers response");

        assert_eq!(pdu.function_code, FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data_len, 5); // Byte Count (0x04) + 4 data bytes
        assert_eq!(pdu.data.as_slice(), &[0x04, 0x12, 0x34, 0x56, 0x78]);
    }

    /// Test case: `Pdu::from_bytes` with a PDU containing the maximum allowed data length.
    ///
    /// The maximum PDU data length is 252 bytes (total PDU size 253 bytes including FC).
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size).
    #[test]
    fn test_pdu_from_bytes_valid_max_data_length() {
        // Max PDU data length is 252 bytes.
        let mut bytes_vec: Vec<u8, 253> = Vec::new();
        let _ = bytes_vec.push(0x03); // Dummy FC (Read Holding Registers)
        for i in 0..252 {
            let _ = bytes_vec.push(i as u8);
        }
        let bytes = bytes_vec.as_slice();
        let pdu = Pdu::from_bytes(bytes).expect("Should parse valid PDU with max data");

        assert_eq!(pdu.function_code, FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data_len, 252);
        assert_eq!(pdu.data.as_slice(), &bytes[1..]);
    }

    /// Test case: `Pdu::from_bytes` with an empty byte slice.
    ///
    /// An empty slice is an invalid PDU as it lacks even a function code.
    #[test]
    fn test_pdu_from_bytes_empty_slice_error() {
        let bytes = [];
        let err = Pdu::from_bytes(&bytes).expect_err("Should return error for empty slice");
        assert_eq!(err, MbusError::InvalidPduLength);
    }

    /// Test case: `Pdu::from_bytes` with an invalid or unsupported function code.
    ///
    /// This checks for `MbusError::UnsupportedFunction` when the function code is not recognized.
    /// Modbus Specification Reference: V1.1b3, Section 5.1 (Public Function Code Definition).
    #[test]
    fn test_pdu_from_bytes_invalid_function_code_error() {
        // 0x00 is not a valid public function code
        let bytes = [0x00, 0x01, 0x02];
        let err = Pdu::from_bytes(&bytes).expect_err("Should return error for invalid FC 0x00");
        assert_eq!(err, MbusError::UnsupportedFunction(0x00));

        // 0xFF is also not a valid public function code
        let bytes = [0xFF, 0x01, 0x02];
        let err = Pdu::from_bytes(&bytes).expect_err("Should return error for invalid FC 0xFF");
        assert_eq!(err, MbusError::UnsupportedFunction(0xFF));
    }

    /// Test case: `Pdu::from_bytes` with a data payload exceeding the maximum allowed length.
    ///
    /// The maximum PDU data length is 252 bytes. This test provides 253 data bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size).
    #[test]
    fn test_pdu_from_bytes_data_too_long_error() {
        // PDU data length > 252 bytes (total PDU length > 253 bytes)
        let mut bytes_vec: Vec<u8, 254> = Vec::new();
        let _ = bytes_vec.push(0x03); // Dummy FC
        for i in 0..253 {
            // 253 data bytes, which is too many
            let _ = bytes_vec.push(i as u8);
        }
        let bytes = bytes_vec.as_slice();
        let err = Pdu::from_bytes(bytes).expect_err("Should return error for too much data");
        assert_eq!(err, MbusError::InvalidPduLength);
    }

    // --- Tests for Pdu::to_bytes ---

    /// Test case: `Pdu::to_bytes` with a PDU that has no data bytes.
    ///
    /// This covers function codes like `ReportServerId` (0x11) which consist only of the function code.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.13 (Report Server ID).
    #[test]
    fn test_pdu_to_bytes_no_data() {
        let pdu = Pdu::new(FunctionCode::ReportServerId, Vec::new(), 0);
        let bytes = pdu.to_bytes().expect("Should convert PDU to bytes");
        assert_eq!(bytes.as_slice(), &[0x11]);
    }

    /// Test case: `Pdu::to_bytes` with a PDU containing a typical data payload.
    ///
    /// Example: `Read Coils` request with address and quantity.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.1 (Read Coils).
    #[test]
    fn test_pdu_to_bytes_with_data() {
        let mut data_vec = Vec::new();
        data_vec.extend_from_slice(&[0x00, 0x00, 0x00, 0x0A]).unwrap(); // Read 10 coils

        let pdu = Pdu::new(FunctionCode::ReadCoils, data_vec, 4);
        let bytes = pdu.to_bytes().expect("Should convert PDU to bytes");
        assert_eq!(bytes.as_slice(), &[0x01, 0x00, 0x00, 0x00, 0x0A]);
    }

    /// Test case: `Pdu::to_bytes` with a PDU containing the maximum allowed data length.
    ///
    /// The maximum PDU data length is 252 bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size).
    #[test]
    fn test_pdu_to_bytes_max_data() {
        let mut data_vec = Vec::new();
        for i in 0..252 {
            data_vec.push(i as u8).unwrap();
        }

        let pdu = Pdu::new(FunctionCode::ReadHoldingRegisters, data_vec, 252);
        let bytes = pdu
            .to_bytes()
            .expect("Should convert PDU to bytes with max data");
        let mut expected_bytes_vec: Vec<u8, 253> = Vec::new();
        let _ = expected_bytes_vec.push(0x03);
        for i in 0..252 {
            let _ = expected_bytes_vec.push(i as u8);
        }
        assert_eq!(bytes.as_slice(), expected_bytes_vec.as_slice());
    }

    // --- Tests for FunctionCode::try_from ---
    /// Test case: `FunctionCode::try_from` with valid `u8` values.
    ///
    /// Verifies that known public function codes are correctly converted to their `FunctionCode` enum variants.
    /// Modbus Specification Reference: V1.1b3, Section 5.1 (Public Function Code Definition).
    #[test]
    fn test_function_code_try_from_valid() {
        assert_eq!(
            FunctionCode::try_from(0x01).unwrap(),
            FunctionCode::ReadCoils
        );
        assert_eq!(
            FunctionCode::try_from(0x08).unwrap(),
            FunctionCode::Diagnostics
        );
        assert_eq!(
            FunctionCode::try_from(0x2B).unwrap(),
            FunctionCode::EncapsulatedInterfaceTransport
        );
        assert_eq!(
            FunctionCode::try_from(0x18).unwrap(),
            FunctionCode::ReadFifoQueue
        );
        assert_eq!(
            FunctionCode::try_from(0x11).unwrap(),
            FunctionCode::ReportServerId
        );
    }

    /// Test case: `FunctionCode::try_from` with invalid or reserved `u8` values.
    ///
    /// Verifies that `MbusError::UnsupportedFunction` is returned for unknown or reserved function code bytes.
    /// Modbus Specification Reference: V1.1b3, Section 5.1 (Public Function Code Definition).
    #[test]
    fn test_function_code_try_from_invalid() {
        let err = FunctionCode::try_from(0x00).expect_err("Should error for invalid FC 0x00");
        assert_eq!(err, MbusError::UnsupportedFunction(0x00));

        let err =
            FunctionCode::try_from(0x09).expect_err("Should error for invalid FC 0x09 (reserved)");
        assert_eq!(err, MbusError::UnsupportedFunction(0x09));

        let err = FunctionCode::try_from(0x64)
            .expect_err("Should error for invalid FC 0x64 (private range, not public)");
        assert_eq!(err, MbusError::UnsupportedFunction(0x64));
    }

    // --- Round-trip tests (from_bytes -> to_bytes) ---

    /// Test case: Round-trip serialization/deserialization for a PDU with no data.
    ///
    /// Converts bytes to PDU and back to bytes, asserting equality with the original.
    /// Modbus Specification Reference: General Modbus PDU structure.
    #[test]
    fn test_pdu_round_trip_no_data() {
        let original_bytes = [0x11]; // ReportServerId
        let pdu = Pdu::from_bytes(&original_bytes).expect("from_bytes failed");
        let new_bytes = pdu.to_bytes().expect("to_bytes failed");
        assert_eq!(original_bytes.as_slice(), new_bytes.as_slice());
    }

    /// Test case: Round-trip serialization/deserialization for a PDU with data.
    ///
    /// Converts bytes to PDU and back to bytes, asserting equality with the original.
    /// Modbus Specification Reference: General Modbus PDU structure.
    #[test]
    fn test_pdu_round_trip_with_data() {
        let original_bytes = [0x01, 0x00, 0x00, 0x00, 0x0A]; // Read Coils
        let pdu = Pdu::from_bytes(&original_bytes).expect("from_bytes failed");
        let new_bytes = pdu.to_bytes().expect("to_bytes failed");
        assert_eq!(original_bytes.as_slice(), new_bytes.as_slice());
    }

    /// Test case: Round-trip serialization/deserialization for a PDU with maximum data length.
    ///
    /// Converts bytes to PDU and back to bytes, asserting equality with the original.
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size).
    #[test]
    fn test_pdu_round_trip_max_data() {
        let mut original_bytes_vec: Vec<u8, 253> = Vec::new();
        let _ = original_bytes_vec.push(0x03); // Read Holding Registers
        for i in 0..252 {
            let _ = original_bytes_vec.push(i as u8);
        }
        let original_bytes = original_bytes_vec.as_slice();

        let pdu = Pdu::from_bytes(original_bytes).expect("from_bytes failed");
        let new_bytes = pdu.to_bytes().expect("to_bytes failed");
        assert_eq!(original_bytes, new_bytes.as_slice());
    }
}
