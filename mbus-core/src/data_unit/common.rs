//! Modbus Common Data Unit Module
//!
//! This module defines the core building blocks of the Modbus protocol, including:
//! - **PDU (Protocol Data Unit)**: The function code and data payload independent of the communication layer.
//! - **ADU (Application Data Unit)**: The complete frame including addressing and error checking.
//! - **MBAP Header**: The specific header used for Modbus TCP transactions.
//! - **Transport Agnostic Logic**: Functions to compile and decompile frames for TCP, RTU, and ASCII modes.
//!
//! The module is designed for `no_std` environments, utilizing `heapless` vectors for fixed-capacity
//! memory management to ensure deterministic behavior in embedded systems.

use crate::errors::MbusError;
use crate::function_codes::public::FunctionCode;
use crate::transport::{SerialMode, TransportType, UnitIdOrSlaveAddr, checksum};
use heapless::Vec;

/// Maximum data length for a PDU (excluding function code)
pub const MAX_PDU_DATA_LEN: usize = 252;

/// Modbus Protocol Identifier (PID)
pub const MODBUS_PROTOCOL_ID: u16 = 0x0000;

/// Maximum length of a Modbus TCP/RTU ADU in bytes.
pub const MAX_ADU_FRAME_LEN_TCP_RTU: usize = 260;

/// Maximum length of a Modbus ASCII ADU in bytes.
pub const MAX_ADU_FRAME_LEN_ASCII: usize = 513;

/// Maximum ADU frame length used by internal buffers.
///
/// - When `serial-ascii` feature is enabled, this is `513` (ASCII upper bound).
/// - Otherwise this is `260` (TCP/RTU upper bound), reducing stack usage.
#[cfg(feature = "serial-ascii")]
pub const MAX_ADU_FRAME_LEN: usize = MAX_ADU_FRAME_LEN_ASCII;

/// Maximum ADU frame length used by internal buffers.
#[cfg(not(feature = "serial-ascii"))]
pub const MAX_ADU_FRAME_LEN: usize = MAX_ADU_FRAME_LEN_TCP_RTU;

#[cfg(test)]
mod frame_len_tests {
    use super::*;

    #[test]
    #[cfg(feature = "serial-ascii")]
    fn test_max_adu_frame_len_ascii_enabled() {
        assert_eq!(MAX_ADU_FRAME_LEN, 513);
    }

    #[test]
    #[cfg(not(feature = "serial-ascii"))]
    fn test_max_adu_frame_len_ascii_disabled() {
        assert_eq!(MAX_ADU_FRAME_LEN, 260);
    }
}

/// Size of the Modbus Application Protocol (MBAP) Header in bytes.
pub const MBAP_HEADER_SIZE: usize = 7;

/// Minimum size of a Modbus RTU ADU in bytes (Address + Function Code + CRC).
pub const MIN_RTU_ADU_LEN: usize = 4;

/// Minimum size of a Modbus ASCII ADU in bytes (Start + Address + Function Code + LRC + End).
pub const MIN_ASCII_ADU_LEN: usize = 9;

/// Size of the Modbus RTU CRC field in bytes.
pub const RTU_CRC_SIZE: usize = 2;

/// Offset of the Transaction ID in the MBAP header.
pub const MBAP_TXN_ID_OFFSET_1B: usize = 0;
/// Offset of the Transaction ID in the MBAP header.
pub const MBAP_TXN_ID_OFFSET_2B: usize = MBAP_TXN_ID_OFFSET_1B + 1;
/// Offset of the Protocol ID in the MBAP header.
pub const MBAP_PROTO_ID_OFFSET_1B: usize = 2;
/// Offset of the Protocol ID in the MBAP header.
pub const MBAP_PROTO_ID_OFFSET_2B: usize = MBAP_PROTO_ID_OFFSET_1B + 1;
/// Offset of  Length field in the MBAP header.
pub const MBAP_LENGTH_OFFSET_1B: usize = 4;
/// Offset of the 2nd byte of the Length field in the MBAP header.
pub const MBAP_LENGTH_OFFSET_2B: usize = MBAP_LENGTH_OFFSET_1B + 1;
/// Offset of the Unit ID in the MBAP header.
pub const MBAP_UNIT_ID_OFFSET: usize = 6;

/// Number of bytes in a Modbus ASCII Start character.
pub const ASCII_START_SIZE: usize = 1;

/// Number of bytes in a Modbus ASCII End sequence (CR LF).
pub const ASCII_END_SIZE: usize = 2;

/// Bit mask used to indicate a Modbus exception in the function code byte.
pub const ERROR_BIT_MASK: u8 = 0x80;

/// Bit mask used to extract the base function code from the function code byte.
pub const FUNCTION_CODE_MASK: u8 = 0x7F;

/// Offset of the high byte of the starting address in PDU data (for read/write functions).
pub const PDU_ADDRESS_OFFSET_1B: usize = 0;
/// Offset of the low byte of the starting address in PDU data.
pub const PDU_ADDRESS_OFFSET_2B: usize = PDU_ADDRESS_OFFSET_1B + 1;
/// Offset of the high byte of the quantity/count in PDU data.
pub const PDU_QUANTITY_OFFSET_1B: usize = 2;
/// Offset of the low byte of the quantity/count in PDU data.
pub const PDU_QUANTITY_OFFSET_2B: usize = PDU_QUANTITY_OFFSET_1B + 1;

/// Checks if the given function code byte indicates an exception (error bit is set).
///
/// # Arguments
/// * `function_code_byte` - The raw function code byte from the PDU.
///
/// # Returns
/// `true` if the highest bit is set, indicating a Modbus exception response.
#[inline]
pub fn is_exception_code(function_code_byte: u8) -> bool {
    function_code_byte & ERROR_BIT_MASK != 0
}

/// Clears the exception bit from the function code byte to retrieve the base function code.
///
/// # Arguments
/// * `function_code_byte` - The raw function code byte from the PDU.
///
/// # Returns
/// The base function code with the error bit cleared.
#[inline]
pub fn clear_exception_bit(function_code_byte: u8) -> u8 {
    function_code_byte & FUNCTION_CODE_MASK
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
    /// Creates a new `SlaveAddress` instance.
    pub fn new(address: u8) -> Result<Self, MbusError> {
        if !(0..=247).contains(&address) {
            return Err(MbusError::InvalidSlaveAddress);
        }
        Ok(Self(address))
    }

    /// Accessor for the slave address.
    pub fn address(&self) -> u8 {
        self.0
    }
}

/// Additional address field for Modbus RTU/TCP messages.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum AdditionalAddress {
    /// The MBAP header for Modbus TCP messages. This includes Transaction ID, Protocol ID, Length, and Unit ID.
    MbapHeader(MbapHeader),
    /// The slave address for Modbus RTU/ASCII messages.
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

    /// Accessor for the Protocol Data Unit (PDU).
    ///
    /// The PDU contains the function code and the data payload, which are
    /// independent of the underlying transport layer (TCP, RTU, or ASCII).
    ///
    pub fn pdu(&self) -> &Pdu {
        &self.pdu
    }

    /// Extracts the target device identifier from the message.
    ///
    /// This method abstracts the difference between TCP (Unit ID) and Serial (Slave Address)
    /// addressing, returning a unified `UnitIdOrSlaveAddr` type.
    ///
    /// # Returns
    /// A `UnitIdOrSlaveAddr` representing the destination or source device.
    pub fn unit_id_or_slave_addr(&self) -> UnitIdOrSlaveAddr {
        match self.additional_address {
            AdditionalAddress::MbapHeader(header) => match header.unit_id {
                0 => UnitIdOrSlaveAddr::new_broadcast_address(),
                unit_id => {
                    UnitIdOrSlaveAddr::try_from(unit_id).unwrap_or(UnitIdOrSlaveAddr::default())
                }
            },
            AdditionalAddress::SlaveAddress(slave_address) => match slave_address.address() {
                0 => UnitIdOrSlaveAddr::new_broadcast_address(),
                address => {
                    UnitIdOrSlaveAddr::try_from(address).unwrap_or(UnitIdOrSlaveAddr::default())
                }
            },
        }
    }

    /// Retrieves the transaction identifier for the message.
    ///
    /// For TCP messages, this returns the ID from the MBAP header.
    /// For Serial (RTU/ASCII) messages, this returns 0 as they are inherently synchronous.
    pub fn transaction_id(&self) -> u16 {
        match self.additional_address {
            AdditionalAddress::MbapHeader(header) => header.transaction_id,
            AdditionalAddress::SlaveAddress(_) => 0,
        }
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

    /// Creates a `ModbusMessage` from its byte representation (ADU).
    ///
    /// This method parses the MBAP header and the PDU from the given byte slice.
    ///
    /// # Arguments
    /// * `bytes` - A byte slice containing the complete Modbus TCP ADU.
    ///
    /// # Returns
    /// `Ok((ModbusMessage, usize))` containing the parsed message and the number of consumed bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, MbusError> {
        // Minimum ADU length: MBAP header + 1 byte Function Code
        if bytes.len() < MBAP_HEADER_SIZE + 1 {
            return Err(MbusError::InvalidAduLength); // Reusing for general invalid length
        }

        // Parse MBAP Header
        // Transaction ID: 2 bytes starting at offset 0
        let transaction_id =
            u16::from_be_bytes([bytes[MBAP_TXN_ID_OFFSET_1B], bytes[MBAP_TXN_ID_OFFSET_2B]]);
        // Protocol ID: 2 bytes starting at offset 2
        let protocol_id = u16::from_be_bytes([
            bytes[MBAP_PROTO_ID_OFFSET_1B],
            bytes[MBAP_PROTO_ID_OFFSET_2B],
        ]);
        // Length: 2 bytes starting at offset 4
        let length =
            u16::from_be_bytes([bytes[MBAP_LENGTH_OFFSET_1B], bytes[MBAP_LENGTH_OFFSET_2B]]);
        // Unit ID: 1 byte at offset 6
        let unit_id = bytes[MBAP_UNIT_ID_OFFSET];

        // Validate Protocol Identifier
        if protocol_id != MODBUS_PROTOCOL_ID {
            return Err(MbusError::BasicParseError); // Invalid protocol ID
        }

        // Validate Length field
        // The length field specifies the number of following bytes, including the Unit ID and PDU.
        // So, actual_pdu_and_unit_id_len = bytes.len() - 6 (MBAP header without length field)
        // And length field value should be actual_pdu_and_unit_id_len
        const INITIAL_FRAME_LEN: usize = MBAP_HEADER_SIZE - 1; // MBAP
        let expected_total_len_from_header = length as usize + INITIAL_FRAME_LEN; // 6 bytes for TID, PID, Length field itself

        // Ensure we have enough bytes in the buffer to form the complete expected frame
        if bytes.len() < expected_total_len_from_header {
            return Err(MbusError::InvalidPduLength);
        }

        // Slice exactly the frame length indicated by the header to support pipelined streams
        let frame_bytes = &bytes[..expected_total_len_from_header];
        // The PDU starts after the MBAP header
        let pdu_bytes_slice = &frame_bytes[MBAP_HEADER_SIZE..];

        // Parse PDU using the existing Pdu::from_bytes method
        let pdu = Pdu::from_bytes(pdu_bytes_slice)?;

        let additional_addr = AdditionalAddress::MbapHeader(MbapHeader {
            transaction_id,
            protocol_id,
            length,
            unit_id,
        });

        Ok(ModbusMessage::new(additional_addr, pdu))
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

    /// Creates a `ModbusMessage` from a raw Modbus RTU byte slice.
    ///
    /// This method validates the RTU frame by checking the minimum length and
    /// verifying the 16-bit CRC (Cyclic Redundancy Check).
    ///
    /// # Arguments
    /// * `frame` - A byte slice containing the complete Modbus RTU ADU.
    ///
    /// # Returns
    /// * `Ok(ModbusMessage)` if the CRC is valid and the PDU is correctly parsed.
    /// * `Err(MbusError)` if the frame is too short, the checksum fails, or the PDU is invalid.
    pub fn from_rtu_bytes(frame: &[u8]) -> Result<Self, MbusError> {
        // RTU Frame: [Slave Address (1)] [PDU (N)] [CRC (2)]
        // Minimum length: MIN_RTU_ADU_LEN (4)
        if frame.len() < MIN_RTU_ADU_LEN {
            return Err(MbusError::InvalidAduLength);
        }

        // The CRC is the last 2 bytes of the frame
        let data_len = frame.len() - RTU_CRC_SIZE;
        let data_to_check = &frame[..data_len];

        // Modbus RTU uses Little-Endian for CRC transmission
        let received_crc = u16::from_le_bytes([frame[data_len], frame[data_len + 1]]);
        let calculated_crc = checksum::crc16(data_to_check);

        // Verify data integrity
        if calculated_crc != received_crc {
            return Err(MbusError::ChecksumError); // CRC Mismatch
        }

        // Extract Slave Address (1st byte)
        let slave_address = SlaveAddress::new(frame[0])?;

        // PDU is from byte 1 to end of data (excluding CRC)
        let pdu_bytes = &data_to_check[1..];
        let pdu = Pdu::from_bytes(pdu_bytes)?;

        Ok(ModbusMessage::new(
            AdditionalAddress::SlaveAddress(slave_address),
            pdu,
        ))
    }

    /// Creates a `ModbusMessage` from a raw Modbus ASCII byte slice.
    ///
    /// This method performs the following validation and transformation steps:
    /// 1. Validates the frame structure (starts with ':', ends with "\r\n").
    /// 2. Decodes the hexadecimal ASCII representation into binary data.
    /// 3. Verifies the Longitudinal Redundancy Check (LRC) checksum.
    /// 4. Parses the resulting binary into a `SlaveAddress` and `Pdu`.
    ///
    /// # Arguments
    /// * `frame` - A byte slice containing the complete Modbus ASCII ADU.
    ///
    /// # Returns
    /// * `Ok(ModbusMessage)` if the frame is valid and checksum matches.
    /// * `Err(MbusError)` for invalid length, malformed hex, or checksum failure.
    pub fn from_ascii_bytes(frame: &[u8]) -> Result<Self, MbusError> {
        // ASCII Frame: [Start (:)] [Address (2)] [PDU (N)] [LRC (2)] [End (\r\n)]
        // Minimum length: MIN_ASCII_ADU_LEN (9)
        if frame.len() < MIN_ASCII_ADU_LEN {
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
        let hex_content = &frame[ASCII_START_SIZE..frame.len() - ASCII_END_SIZE];
        if !hex_content.len().is_multiple_of(2) {
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

        Ok(ModbusMessage::new(
            AdditionalAddress::SlaveAddress(slave_address),
            pdu,
        ))
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
            data,             // Ensure the heapless::Vec is moved here
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

    /// Reads the starting address from the PDU data.
    ///
    /// # Valid for
    /// **Read request frames only**: FC01 (Read Coils), FC02 (Read Discrete Inputs),
    /// FC03 (Read Holding Registers), FC04 (Read Input Registers).
    ///
    /// # Returns
    /// The starting address as a big-endian 16-bit value from PDU bytes 0-1.
    ///
    /// # Errors
    /// Returns `InvalidPduLength` if `data_len < 2`.
    ///
    /// # Warnings
    /// **DO NOT CALL on**:
    /// - **Responses**: FC03/FC04 responses have format `[ByteCount][RegisterData]` with no address.
    /// - **Write requests**: FC05, FC06, FC15, FC16 have different structures.
    /// - **Other function codes**: Each has its own unique PDU layout.
    ///
    /// Calling on unsupported frames will silently read incorrect data.
    pub fn address_read_frame(&self) -> Result<u16, MbusError> {
        if self.data_len < 2 {
            return Err(MbusError::InvalidPduLength);
        }
        Ok(u16::from_be_bytes([
            self.data[PDU_ADDRESS_OFFSET_1B],
            self.data[PDU_ADDRESS_OFFSET_2B],
        ]))
    }

    /// Reads the quantity/count from the PDU data.
    ///
    /// # Valid for
    /// **Read request frames only**: FC01 (Read Coils), FC02 (Read Discrete Inputs),
    /// FC03 (Read Holding Registers), FC04 (Read Input Registers).
    ///
    /// # Returns
    /// The quantity/count as a big-endian 16-bit value from PDU bytes 2-3.
    ///
    /// # Errors
    /// Returns `InvalidPduLength` if `data_len < 4`.
    ///
    /// # Warnings
    /// **DO NOT CALL on**:
    /// - **Responses**: FC03/FC04 responses have format `[ByteCount][RegisterData]` with no quantity.
    /// - **Write requests**: FC05, FC06, FC15, FC16 have different structures.
    /// - **Other function codes**: Each has its own unique PDU layout.
    ///
    /// Calling on unsupported frames will silently read incorrect data.
    pub fn quantity_from_read_frame(&self) -> Result<u16, MbusError> {
        if self.data_len < 4 {
            return Err(MbusError::InvalidPduLength);
        }
        Ok(u16::from_be_bytes([
            self.data[PDU_QUANTITY_OFFSET_1B],
            self.data[PDU_QUANTITY_OFFSET_2B],
        ]))
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
    ///
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

        let error_code = if is_exception_code(bytes[0]) {
            Some(bytes[1]) // The second byte is the exception code for error responses
        } else {
            None
        };
        let function_code = clear_exception_bit(bytes[0]); // Mask out the error bit to get the actual function code

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
        TransportType::StdSerial(serial_mode) | TransportType::CustomSerial(serial_mode) => {
            let slave_address = SlaveAddress(unit_id);
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
                    ModbusMessage::new(AdditionalAddress::SlaveAddress(slave_address), pdu)
                        .to_ascii_bytes()?
                }
            };

            Ok(adu_bytes)
        }
    }
}

/// Decodes a raw transport frame into a ModbusMessage based on the transport type.
pub fn decompile_adu_frame(
    frame: &[u8],
    transport_type: TransportType,
) -> Result<ModbusMessage, MbusError> {
    match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // Parse MBAP header and PDU
            ModbusMessage::from_bytes(frame)
        }
        TransportType::StdSerial(serial_mode) | TransportType::CustomSerial(serial_mode) => {
            match serial_mode {
                SerialMode::Rtu => ModbusMessage::from_rtu_bytes(frame),
                SerialMode::Ascii => ModbusMessage::from_ascii_bytes(frame),
            }
        }
    }
}

/// Derives the expected total length of a Modbus frame from its initial bytes.
///
/// This function is used by stream-based transports to determine if a complete
/// Application Data Unit (ADU) has been received before attempting full decompression.
///
/// # Arguments
/// * `frame` - The raw byte buffer containing the partial or full frame.
/// * `transport_type` - The Modbus variant (TCP, RTU, or ASCII).
///
/// # Returns
/// * `Some(usize)` - The calculated total length of the frame if enough metadata is present.
/// * `None` - If the buffer is too short to determine the length.
pub fn derive_length_from_bytes(frame: &[u8], transport_type: TransportType) -> Option<usize> {
    match transport_type {
        TransportType::StdTcp | TransportType::CustomTcp => {
            // TCP (MBAP) requires at least 6 bytes to read the 'Length' field.
            // MBAP structure: TID(2), PID(2), Length(2), UnitID(1)
            if frame.len() < 6 {
                return None;
            }

            // In Modbus TCP, the Protocol ID MUST be 0x0000.
            // If it's not, this is garbage data. We return a huge length to trigger a parse error
            // downstream and force the window to slide and resync.
            let protocol_id = u16::from_be_bytes([frame[2], frame[3]]);
            if protocol_id != MODBUS_PROTOCOL_ID {
                return Some(usize::MAX);
            }

            // The Length field in MBAP (offset 4) counts all following bytes (UnitID + PDU).
            // Total ADU = 6 bytes (Header up to Length) + value of Length field.
            let length_field = u16::from_be_bytes([frame[4], frame[5]]) as usize;
            Some(6 + length_field)
        }
        TransportType::StdSerial(SerialMode::Rtu)
        | TransportType::CustomSerial(SerialMode::Rtu) => {
            if frame.len() < 2 {
                return None;
            }

            let fc = frame[1];

            // Exception responses are universally 5 bytes: [ID][FC+0x80][ExcCode][CRC_L][CRC_H]
            if is_exception_code(fc) {
                return Some(5);
            }

            // Helper: Opportunistically verifies if a potential frame boundary has a valid CRC.
            // This allows us to disambiguate Requests vs Responses dynamically as the stream arrives.
            let check_crc = |len: usize| -> bool {
                if frame.len() >= len && len >= MIN_RTU_ADU_LEN {
                    let data_len = len - RTU_CRC_SIZE;
                    let received_crc = u16::from_le_bytes([frame[data_len], frame[data_len + 1]]);
                    checksum::crc16(&frame[..data_len]) == received_crc
                } else {
                    false
                }
            };

            get_byte_count_from_frame(frame, fc, check_crc)
        }
        TransportType::StdSerial(SerialMode::Ascii)
        | TransportType::CustomSerial(SerialMode::Ascii) => {
            // ASCII frames are delimited by ':' and '\r\n'.
            // We scan for the end-of-frame marker.
            if frame.len() < MIN_ASCII_ADU_LEN {
                return None;
            }

            // Fast linear scan for the LF character which terminates the frame
            frame.iter().position(|&b| b == b'\n').map(|pos| pos + 1)
        }
    }
}

fn get_byte_count_from_frame(
    frame: &[u8],
    fc: u8,
    check_crc: impl Fn(usize) -> bool,
) -> Option<usize> {
    let mut candidates = heapless::Vec::<usize, 4>::new();
    let mut min_needed = usize::MAX;

    // Helper function to safely calculate dynamic lengths based on a byte in the frame.
    // We pass candidates as a mutable reference to avoid "multiple mutable borrow" errors
    // that occur when a closure captures a mutable variable and is called multiple times.
    let mut add_dyn = |cands: &mut heapless::Vec<usize, 4>, offset: usize, base: usize| {
        if frame.len() > offset {
            let _ = cands.push(base + frame[offset] as usize);
        } else {
            min_needed = core::cmp::min(min_needed, offset + 1);
        }
    };

    // Map structural candidates (Requests and Responses combined) based on Modbus definitions
    match fc {
        1..=4 => {
            let _ = candidates.push(8);
            add_dyn(&mut candidates, 2, 5);
        }
        5 | 6 | 8 => {
            let _ = candidates.push(8);
        }
        7 => {
            let _ = candidates.push(4);
            let _ = candidates.push(5);
        }
        11 => {
            let _ = candidates.push(4);
            let _ = candidates.push(8);
        }
        12 | 17 => {
            let _ = candidates.push(4);
            add_dyn(&mut candidates, 2, 5);
        }
        15 | 16 => {
            let _ = candidates.push(8);
            add_dyn(&mut candidates, 6, 9);
        }
        20 | 21 => add_dyn(&mut candidates, 2, 5),
        22 => {
            let _ = candidates.push(10);
        }
        23 => {
            add_dyn(&mut candidates, 2, 5);
            add_dyn(&mut candidates, 10, 13);
        }
        24 => {
            let _ = candidates.push(6);
            if frame.len() >= 4 {
                let byte_count = u16::from_be_bytes([frame[2], frame[3]]) as usize;
                let _ = candidates.push(6 + byte_count);
            } else {
                min_needed = core::cmp::min(min_needed, 4);
            }
        }
        43 => {
            if check_crc(7) {
                return Some(7);
            }
            // Response is unpredictable. Scan opportunistically forwards to support pipelined frames.
            for len in MIN_RTU_ADU_LEN..=frame.len() {
                if check_crc(len) {
                    return Some(len);
                }
            }
            return None;
        }
        _ => {
            for len in MIN_RTU_ADU_LEN..=frame.len() {
                if check_crc(len) {
                    return Some(len);
                }
            }
            return None;
        }
    }

    // 1. Opportunistic CRC checks to lock in an exact frame boundary
    for &len in &candidates {
        if check_crc(len) {
            return Some(len);
        }
    }

    // 2. If no CRC matched yet, determine the max length we might need to wait for
    let max_candidate = candidates.iter().copied().max().unwrap_or(0);
    let target = if min_needed != usize::MAX {
        core::cmp::max(min_needed, max_candidate)
    } else {
        max_candidate
    };

    if target > 0 { Some(target) } else { None }
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
        assert_eq!(err, MbusError::InvalidAduLength);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_valid() {
        // Frame: 01 03 00 6B 00 03 74 17 (CRC LE)
        let frame = [0x01, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x74, 0x17];
        let msg = decompile_adu_frame(&frame, TransportType::StdSerial(SerialMode::Rtu))
            .expect("Valid RTU");
        assert_eq!(msg.function_code(), FunctionCode::ReadHoldingRegisters);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_too_short() {
        let frame = [0x01, 0x02, 0x03];
        let err = decompile_adu_frame(&frame, TransportType::StdSerial(SerialMode::Rtu))
            .expect_err("Too short");
        assert_eq!(err, MbusError::InvalidAduLength);
    }

    #[test]
    fn test_decompile_adu_frame_rtu_crc_mismatch() {
        let frame = [0x01, 0x03, 0x00, 0x6B, 0x00, 0x03, 0x00, 0x00]; // Bad CRC
        let err = decompile_adu_frame(&frame, TransportType::StdSerial(SerialMode::Rtu))
            .expect_err("CRC mismatch");
        assert_eq!(err, MbusError::ChecksumError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_valid() {
        // :010300000001FB\r\n
        let frame = b":010300000001FB\r\n";
        let msg = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect("Valid ASCII");
        assert_eq!(msg.function_code(), FunctionCode::ReadHoldingRegisters);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_too_short() {
        let frame = b":123\r\n";
        let err = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect_err("Too short");
        assert_eq!(err, MbusError::InvalidAduLength);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_missing_start() {
        let frame = b"010300000001FB\r\n";
        let err = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect_err("Missing start");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_missing_end() {
        let frame = b":010300000001FB\r"; // Missing \n
        let err = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect_err("Missing end");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_odd_hex() {
        let frame = b":010300000001F\r\n"; // Odd length hex
        let err = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect_err("Odd hex");
        assert_eq!(err, MbusError::BasicParseError);
    }

    #[test]
    fn test_decompile_adu_frame_ascii_lrc_mismatch() {
        let frame = b":01030000000100\r\n"; // LRC 00 is wrong, should be FB
        let err = decompile_adu_frame(frame, TransportType::StdSerial(SerialMode::Ascii))
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
        let err = decompile_adu_frame(&frame, TransportType::StdSerial(SerialMode::Ascii))
            .expect_err("Buffer overflow");
        assert_eq!(err, MbusError::BufferTooSmall);
    }

    // --- Tests for derive_length_from_bytes ---

    #[test]
    fn test_derive_length_tcp() {
        // TCP frame requires minimum 6 bytes to read the length offset.
        let short_frame = [0x00, 0x01, 0x00, 0x00, 0x00];
        assert_eq!(
            derive_length_from_bytes(&short_frame, TransportType::StdTcp),
            None
        );

        // TCP MBAP: TID(2) PID(2) LEN(2) = 0x0006. Total length should be 6 + 6 = 12.
        let full_frame = [
            0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01,
        ];
        assert_eq!(
            derive_length_from_bytes(&full_frame, TransportType::StdTcp),
            Some(12)
        );

        // TCP MBAP with invalid Protocol ID should return usize::MAX to trigger garbage disposal.
        let garbage_frame = [
            0x00, 0x01, 0xAA, 0xBB, 0x00, 0x06, 0x01, 0x03, 0x00, 0x00, 0x00, 0x01,
        ];
        assert_eq!(
            derive_length_from_bytes(&garbage_frame, TransportType::StdTcp),
            Some(usize::MAX)
        );
    }

    #[test]
    fn test_derive_length_rtu_fixed() {
        // FC 5 (Write Single Coil) is uniformly 8 bytes for requests and responses.
        let request = [0x01, 0x05, 0x00, 0x0A, 0xFF, 0x00, 0x00, 0x00];
        assert_eq!(
            derive_length_from_bytes(&request, TransportType::StdSerial(SerialMode::Rtu)),
            Some(8)
        );
    }

    #[test]
    fn test_derive_length_rtu_dynamic() {
        // Read Holding Registers Response (FC 03)
        // Schema: Address(1) + FC(03) + ByteCount(2) + Data(0x12, 0x34) + CRC(2)
        let mut resp = [0x01, 0x03, 0x02, 0x12, 0x34, 0x00, 0x00];
        let crc = checksum::crc16(&resp[..5]);
        let crc_bytes = crc.to_le_bytes();
        resp[5] = crc_bytes[0];
        resp[6] = crc_bytes[1];

        // Opportunistic CRC should instantly lock on an exact length of 7.
        assert_eq!(
            derive_length_from_bytes(&resp, TransportType::StdSerial(SerialMode::Rtu)),
            Some(7)
        );

        // A partial frame (4 bytes) should predict up to 8 (max candidate comparison logic).
        assert_eq!(
            derive_length_from_bytes(&resp[..4], TransportType::StdSerial(SerialMode::Rtu)),
            Some(8)
        );
    }

    #[test]
    fn test_derive_length_rtu_exception() {
        // Exception responses are universally 5 bytes.
        let exception = [0x01, 0x81, 0x02, 0x00, 0x00];
        assert_eq!(
            derive_length_from_bytes(&exception, TransportType::StdSerial(SerialMode::Rtu)),
            Some(5)
        );
    }

    #[test]
    fn test_derive_length_rtu_forward_scan() {
        // Build a frame with unknown/custom Function Code (e.g. 0x44)
        let mut custom_frame = [0x01, 0x44, 0xAA, 0xBB, 0x00, 0x00];
        let crc = checksum::crc16(&custom_frame[..4]);
        let crc_bytes = crc.to_le_bytes();
        custom_frame[4] = crc_bytes[0];
        custom_frame[5] = crc_bytes[1];

        assert_eq!(
            derive_length_from_bytes(&custom_frame, TransportType::StdSerial(SerialMode::Rtu)),
            Some(6)
        );

        // Without the valid CRC, it will continuously scan forward but yield None if unmatched.
        assert_eq!(
            derive_length_from_bytes(
                &custom_frame[..4],
                TransportType::StdSerial(SerialMode::Rtu)
            ),
            None
        );
    }

    #[test]
    fn test_derive_length_ascii() {
        let frame = b":010300000001FB\r\n";
        assert_eq!(
            derive_length_from_bytes(frame, TransportType::StdSerial(SerialMode::Ascii)),
            Some(17)
        );

        let partial = b":010300000001F";
        assert_eq!(
            derive_length_from_bytes(partial, TransportType::StdSerial(SerialMode::Ascii)),
            None
        );
    }
}
