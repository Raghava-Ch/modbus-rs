use crate::data_unit::tcp::ModbusTcpMessage;
use crate::errors::MbusError;
use crate::function_codes::public::{
    DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode,
};
use crate::transport::{SerialMode, TransportType, checksum};
use heapless::Vec;

pub const MAX_PDU_DATA_LEN: usize = 252; // Maximum data length for a PDU (excluding function code)
// Maximum length of an ADU (MBAP header + PDU)
// Maximum length of an ADU (ASCII mode requires 513 bytes)
pub const MAX_ADU_FRAME_LEN: usize = 513;

const ERROR_BIT_MASK: u8 = 0x80;
const FUNCTION_CODE_MASK: u8 = 0x7F;

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
    /// Optional error code for exception responses (only valid if function_code indicates an error).
    error_code: Option<u8>,
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

/// Represents a Modbus slave address for RTU/ASCII messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlaveAddress(u8);

impl SlaveAddress {
    pub fn new(address: u8) -> Result<Self, MbusError> {
        if !(1..=247).contains(&address) {
            return Err(MbusError::InvalidSlaveAddress);
        }
        Ok(Self(address))
    }

    pub fn address(&self) -> u8 {
        self.0
    }
}

/// Additional address field for Modbus RTU/TCP messages.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum AdditionalAddress {
    /// The additional address field used in certain Modbus function codes.
    MbapHeader(MbapHeader),
    SlaveAddress(SlaveAddress),
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
    /// `Ok(Vec<u8, MAX_ADU_LEN>)` containing the ADU bytes, or an `MbusError` if
    /// the message cannot be serialized.
    pub fn to_bytes(&self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
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
                    .push(address.address())
                    .map_err(|_| MbusError::Unexpected)?;
            }
        }

        let pdu_bytes = self.pdu.to_bytes()?;
        adu_bytes
            .extend_from_slice(&pdu_bytes)
            .map_err(|_| MbusError::Unexpected)?;

        Ok(adu_bytes)
    }

    /// Converts the `ModbusMessage` into its ASCII ADU byte representation.
    ///
    /// This method serializes the message to binary, calculates the LRC,
    /// and then encodes the result into Modbus ASCII format (Start ':', Hex, End CR LF).
    ///
    /// # Returns
    /// `Ok(Vec<u8, MAX_ADU_FRAME_LEN>)` containing the ASCII ADU bytes.
    ///
    /// # Errors
    /// Returns `MbusError::BufferTooSmall` if the resulting ASCII frame exceeds `MAX_ADU_FRAME_LEN`.
    pub fn to_ascii_bytes(&self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let mut binary_data = self.to_bytes()?;

        // Calculate LRC on Address + PDU
        let lrc = checksum::lrc(&binary_data);

        // Append LRC to binary data temporarily to iterate over it
        binary_data
            .push(lrc)
            .map_err(|_| MbusError::BufferTooSmall)?;

        let mut ascii_data = Vec::new();

        // Start character ':'
        ascii_data
            .push(b':')
            .map_err(|_| MbusError::BufferTooSmall)?;

        for byte in binary_data {
            let high = (byte >> 4) & 0x0F;
            let low = byte & 0x0F;

            ascii_data
                .push(nibble_to_hex(high))
                .map_err(|_| MbusError::BufferTooSmall)?;
            ascii_data
                .push(nibble_to_hex(low))
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        // End characters CR LF
        ascii_data
            .push(b'\r')
            .map_err(|_| MbusError::BufferTooSmall)?;
        ascii_data
            .push(b'\n')
            .map_err(|_| MbusError::BufferTooSmall)?;

        Ok(ascii_data)
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
    pub fn new(
        function_code: FunctionCode,
        data: heapless::Vec<u8, MAX_PDU_DATA_LEN>,
        data_len: u8,
    ) -> Self {
        Self {
            function_code,
            error_code: None, // Default to None for normal responses; can be set for exceptions
            data: data,       // Ensure the heapless::Vec is moved here
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

    /// Accessor for the error code from the PDU.
    pub fn error_code(&self) -> Option<u8> {
        self.error_code
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
        pdu_bytes
            .push(self.function_code as u8)
            .map_err(|_| MbusError::Unexpected)?; // Function code (1 byte)

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
        if bytes.is_empty() || bytes.len() < 2 {
            return Err(MbusError::InvalidPduLength);
        }

        let error_code = if bytes[0] & ERROR_BIT_MASK != 0 {
            Some(bytes[1]) // The second byte is the exception code for error responses
        } else {
            None
        };
        let function_code = bytes[0] & FUNCTION_CODE_MASK; // Mask out the error bit to get the actual function code

        let function_code = FunctionCode::try_from(function_code)?;

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
            error_code,
            data,
            data_len: data_len as u8,
        })
    }
}

/// Helper to build the ADU from PDU based on transport type.
pub fn compile_adu_frame(
    txn_id: u16,
    unit_id: u8,
    pdu: Pdu,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            let pdu_bytes_len = pdu.to_bytes()?.len() as u16;
            let mbap_header = MbapHeader::new(txn_id, pdu_bytes_len + 1, unit_id);
            ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu).to_bytes()
        }
        TransportType::StdSerial(slave_address, serial_mode)
        | TransportType::CustomSerial(slave_address, serial_mode) => {
            let adu_bytes = match serial_mode {
                SerialMode::Rtu => {
                    let mut adu_bytes =
                        ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_address), pdu)
                            .to_bytes()?;
                    // Calculate the 16-bit CRC for the Slave Address + PDU.
                    let crc16 = checksum::crc16(adu_bytes.as_slice());
                    // Modbus RTU transmits CRC in Little-Endian (LSB first) according to the spec.
                    let crc_bytes = crc16.to_le_bytes();
                    adu_bytes
                        .extend_from_slice(&crc_bytes)
                        .map_err(|_| MbusError::Unexpected)?;

                    adu_bytes
                }
                SerialMode::Ascii => {
                    let mut adu_bytes =
                        ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_address), pdu)
                            .to_ascii_bytes()?;
                    let lrc = checksum::lrc(&adu_bytes);
                    adu_bytes.push(lrc).map_err(|_| MbusError::Unexpected)?;
                    adu_bytes
                }
            };

            Ok(adu_bytes)
        }
    }
}

/// Decodes a raw transport frame into a ModbusTcpMessage based on the transport type.
pub fn decompile_adu_frame(
    frame: &[u8],
    transport_type: TransportType,
) -> Result<ModbusMessage, MbusError> {
    let message = match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // Parse MBAP header and PDU
            match ModbusTcpMessage::from_adu_bytes(frame) {
                Ok(msg) => {
                    let additional_address =
                        AdditionalAddress::MbapHeader(msg.mbap_header().clone());
                    ModbusMessage {
                        additional_address: additional_address,
                        pdu: msg.pdu().clone(),
                    } // Successfully decoded the frame.
                }
                // If decoding fails, the frame is dropped.
                Err(_e) => {
                    return Err(MbusError::BasicParseError);
                }
            }
        }
        TransportType::StdSerial(_slave_address, serial_mode)
        | TransportType::CustomSerial(_slave_address, serial_mode) => {
            match serial_mode {
                SerialMode::Rtu => {
                    // RTU Frame: [Slave Address (1)] [PDU (N)] [CRC (2)]
                    // Minimum length: 1 (Addr) + 1 (FC) + 2 (CRC) = 4
                    if frame.len() < 4 {
                        return Err(MbusError::InvalidAduLength);
                    }

                    let data_len = frame.len() - 2;
                    let data_to_check = &frame[..data_len];
                    let received_crc = u16::from_le_bytes([frame[data_len], frame[data_len + 1]]);

                    let calculated_crc = checksum::crc16(data_to_check);

                    if calculated_crc != received_crc {
                        return Err(MbusError::ChecksumError); // CRC Mismatch
                    }

                    let slave_address = SlaveAddress::new(frame[0])?;
                    // PDU is from byte 1 to end of data (excluding CRC)
                    let pdu_bytes = &frame[1..data_len];
                    let pdu = Pdu::from_bytes(pdu_bytes)?;

                    ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_address), pdu)
                }
                SerialMode::Ascii => {
                    // ASCII Frame: [Start (:)] [Address (2)] [PDU (N)] [LRC (2)] [End (\r\n)]
                    // Minimum length: 1 + 2 + 2 (FC) + 2 + 2 = 9 bytes
                    if frame.len() < 9 {
                        return Err(MbusError::InvalidAduLength);
                    }

                    // Check Start and End characters
                    if frame[0] != b':' {
                        return Err(MbusError::BasicParseError); // Missing start char
                    }
                    if frame[frame.len() - 2] != b'\r' || frame[frame.len() - 1] != b'\n' {
                        return Err(MbusError::BasicParseError); // Missing end chars
                    }

                    // Extract Hex content (excluding ':' and '\r\n')
                    let hex_content = &frame[1..frame.len() - 2];
                    if hex_content.len() % 2 != 0 {
                        return Err(MbusError::BasicParseError); // Odd length hex string
                    }

                    // Decode Hex to Binary
                    // Max binary length = (513 - 3) / 2 = 255. Using 260 for safety.
                    let mut binary_data: Vec<u8, 260> = Vec::new();
                    for chunk in hex_content.chunks(2) {
                        let byte = hex_pair_to_byte(chunk[0], chunk[1])?;
                        binary_data
                            .push(byte)
                            .map_err(|_| MbusError::BufferTooSmall)?;
                    }

                    // Binary structure: [Slave Address (1)] [PDU (N)] [LRC (1)]
                    if binary_data.len() < 2 {
                        return Err(MbusError::InvalidAduLength);
                    }

                    let data_len = binary_data.len() - 1;
                    let data_to_check = &binary_data[..data_len];
                    let received_lrc = binary_data[data_len];

                    let calculated_lrc = checksum::lrc(data_to_check);

                    if calculated_lrc != received_lrc {
                        return Err(MbusError::ChecksumError); // LRC Mismatch
                    }

                    let slave_address = SlaveAddress::new(binary_data[0])?;
                    let pdu_bytes = &binary_data[1..data_len];
                    let pdu = Pdu::from_bytes(pdu_bytes)?;

                    ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_address), pdu)
                }
            }
        }
    };
    Ok(message)
}

/// Helper function to convert a 4-bit nibble to its ASCII hex representation.
fn nibble_to_hex(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        10..=15 => b'A' + (nibble - 10),
        _ => b'?', // Should not happen for a valid nibble
    }
}

/// Helper function to convert a hex character to its 4-bit nibble value.
fn hex_char_to_nibble(c: u8) -> Result<u8, MbusError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        _ => Err(MbusError::BasicParseError),
    }
}

/// Helper function to convert two hex characters to a byte.
fn hex_pair_to_byte(high: u8, low: u8) -> Result<u8, MbusError> {
    let h = hex_char_to_nibble(high)?;
    let l = hex_char_to_nibble(low)?;
    Ok((h << 4) | l)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_codes::public::FunctionCode;
    use heapless::Vec;

    // --- Tests for Pdu::from_bytes ---

    /// Test case: `Pdu::from_bytes` with a PDU that has no data bytes (only FC).
    ///
    /// According to the implementation, a PDU must be at least 2 bytes.
    #[test]
    fn test_pdu_from_bytes_invalid_no_data() {
        let bytes = [0x11];
        let err = Pdu::from_bytes(&bytes).expect_err("Should return error for PDU with only FC");
        assert_eq!(err, MbusError::InvalidPduLength);
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
        assert_eq!(err, MbusError::UnsupportedFunction(0x7F));
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
        data_vec
            .extend_from_slice(&[0x00, 0x00, 0x00, 0x0A])
            .unwrap(); // Read 10 coils

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

    /// Test case: `ModbusMessage::to_bytes` for a Modbus TCP message.
    ///
    /// Verifies that a `ModbusMessage` with an `MbapHeader` and `Pdu` is correctly
    /// serialized into its ADU byte representation.
    #[test]
    fn test_modbus_message_to_bytes_tcp() {
        let mbap_header = MbapHeader {
            transaction_id: 0x1234,
            protocol_id: 0x0000,
            length: 0x0005, // Length of PDU (FC + Data) + Unit ID
            unit_id: 0x01,
        };

        let mut pdu_data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        pdu_data_vec.extend_from_slice(&[0x00, 0x00, 0x00]).unwrap();

        let pdu = Pdu::new(
            FunctionCode::ReadHoldingRegisters,
            pdu_data_vec,
            3, // 3 data bytes
        );

        let modbus_message = ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu);
        let adu_bytes = modbus_message
            .to_bytes()
            .expect("Failed to serialize ModbusMessage");

        #[rustfmt::skip]
        let expected_adu: [u8; 11] = [
            0x12, 0x34, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x05, // Length (PDU length (FC + Data) + 1 byte Unit ID = (1 + 3) + 1 = 5)
            0x01,       // Unit ID
            0x03,       // Function Code (Read Holding Registers)
            0x00, 0x00, 0x00, // Data
        ];

        assert_eq!(adu_bytes.as_slice(), &expected_adu);
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

    // --- Tests for ModbusMessage::to_ascii_bytes ---

    /// Test case: `ModbusMessage::to_ascii_bytes` with a valid message.
    ///
    /// Verifies correct ASCII encoding and LRC calculation.
    /// Request: Slave 1, Read Coils (FC 01), Start 0, Qty 10.
    /// Binary: 01 01 00 00 00 0A
    /// LRC: -(01+01+0A) = -12 = F4
    /// ASCII: :01010000000AF4\r\n
    #[test]
    fn test_modbus_message_to_ascii_bytes_valid() {
        let slave_addr = SlaveAddress::new(1).unwrap();
        let mut data = Vec::new();
        data.extend_from_slice(&[0x00, 0x00, 0x00, 0x0A]).unwrap();
        let pdu = Pdu::new(FunctionCode::ReadCoils, data, 4);
        let message = ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_addr), pdu);

        let ascii_bytes = message
            .to_ascii_bytes()
            .expect("Failed to convert to ASCII");

        let expected = b":01010000000AF4\r\n";
        assert_eq!(ascii_bytes.as_slice(), expected);
    }

    /// Test case: `ModbusMessage::to_ascii_bytes` boundary check.
    ///
    /// Case 1: Data len 125. Total ASCII = 1 + (1+1+125+1)*2 + 2 = 1 + 256 + 2 = 259.
    /// This fits comfortably within MAX_ADU_FRAME_LEN (513).
    #[test]
    fn test_modbus_message_to_ascii_bytes_max_capacity() {
        let slave_addr = SlaveAddress::new(1).unwrap();
        let mut data = Vec::new();
        for _ in 0..125 {
            data.push(0xAA).unwrap();
        }
        let pdu = Pdu::new(FunctionCode::ReadHoldingRegisters, data, 125);
        let message = ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_addr), pdu);

        let ascii_bytes = message.to_ascii_bytes().expect("Should fit in buffer");
        assert_eq!(ascii_bytes.len(), 259);
    }

    /// Test case: `ModbusMessage::to_ascii_bytes` with large payload.
    ///
    /// Case 2: Data len 126. Total ASCII = 1 + (1+1+126+1)*2 + 2 = 1 + 258 + 2 = 261.
    /// This should NOT fail with MAX_ADU_FRAME_LEN = 513.
    #[test]
    fn test_modbus_message_to_ascii_bytes_large_payload() {
        let slave_addr = SlaveAddress::new(1).unwrap();
        let mut data = Vec::new();
        for _ in 0..126 {
            data.push(0xAA).unwrap();
        }
        let pdu = Pdu::new(FunctionCode::ReadHoldingRegisters, data, 126);
        let message = ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_addr), pdu);

        let ascii_bytes = message.to_ascii_bytes().expect("Should fit in buffer");
        assert_eq!(ascii_bytes.len(), 261);
    }

    // --- Tests for decompile_adu_frame ---

    #[test]
    fn test_decompile_adu_frame_tcp_valid() {
        let frame = [
            0x12, 0x34, // TID
            0x00, 0x00, // PID
            0x00, 0x06, // Length
            0x01, // Unit ID
            0x03, // FC
            0x00, 0x01, 0x00, 0x02, // Data
        ];
        let msg = decompile_adu_frame(&frame, TransportType::StdTcp)
            .expect("Should decode valid TCP frame");
        assert_eq!(msg.function_code(), FunctionCode::ReadHoldingRegisters);
        if let AdditionalAddress::MbapHeader(header) = msg.additional_address {
            assert_eq!(header.transaction_id, 0x1234);
        } else {
            panic!("Expected MbapHeader");
        }
    }

    #[test]
    fn test_decompile_adu_frame_tcp_invalid() {
        let frame = [0x00]; // Too short
        let err = decompile_adu_frame(&frame, TransportType::StdTcp).expect_err("Should fail");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_valid() {
        // Frame: 01 03 00 6B 00 03 74 17 (CRC LE)
        let frame = [0x01, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x74, 0x17];
        let slave_addr = SlaveAddress::new(1).unwrap();
        let msg = decompile_adu_frame(
            &frame,
            TransportType::StdSerial(slave_addr, SerialMode::Rtu),
        )
        .expect("Valid RTU");
        assert_eq!(msg.function_code(), FunctionCode::ReadHoldingRegisters);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_too_short() {
        let frame = [0x01, 0x02, 0x03];
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            &frame,
            TransportType::StdSerial(slave_addr, SerialMode::Rtu),
        )
        .expect_err("Too short");
        assert_eq!(err, MbusError::InvalidAduLength);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_crc_mismatch() {
        let frame = [0x01, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x00, 0x00]; // Bad CRC
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            &frame,
            TransportType::StdSerial(slave_addr, SerialMode::Rtu),
        )
        .expect_err("CRC mismatch");
        assert_eq!(err, MbusError::ChecksumError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_valid() {
        // :010300000001FB\r\n
        let frame = b":010300000001FB\r\n";
        let slave_addr = SlaveAddress::new(1).unwrap();
        let msg = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect("Valid ASCII");
        assert_eq!(msg.function_code(), FunctionCode::ReadHoldingRegisters);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_too_short() {
        let frame = b":123\r\n";
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("Too short");
        assert_eq!(err, MbusError::InvalidAduLength);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_missing_start() {
        let frame = b"010300000001FB\r\n";
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("Missing start");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_missing_end() {
        let frame = b":010300000001FB\r"; // Missing \n
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("Missing end");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_odd_hex() {
        let frame = b":010300000001F\r\n"; // Odd length hex
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("Odd hex");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_lrc_mismatch() {
        let frame = b":01030000000100\r\n"; // LRC 00 is wrong, should be FB
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("LRC mismatch");
        assert_eq!(err, MbusError::ChecksumError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_buffer_overflow() {
        // Construct a frame that decodes to 261 bytes.
        let mut frame = Vec::<u8, 600>::new();
        frame.push(b':').unwrap();
        for _ in 0..261 {
            frame.extend_from_slice(b"00").unwrap();
        }
        frame.extend_from_slice(b"\r\n").unwrap();
        let slave_addr = SlaveAddress::new(1).unwrap();
        let err = decompile_adu_frame(
            &frame,
            TransportType::StdSerial(slave_addr, SerialMode::Ascii),
        )
        .expect_err("Buffer overflow");
        assert_eq!(err, MbusError::BufferTooSmall);
    }
}
