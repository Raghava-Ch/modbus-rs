//! Modbus Error Module
//!
//! This module defines the centralized error handling for the Modbus stack.
//! It provides the [`MbusError`] enum, which covers a wide range of error conditions
//! including protocol-specific exceptions, parsing failures, transport-layer issues,
//! and buffer management errors.
//!
//! The error types are designed to be compatible with `no_std` environments while
//! providing descriptive error messages through the `Display` trait implementation.
//!
//! Modbus Specification Reference: V1.1b3, Section 7 (MODBUS Exception Responses).

#[cfg(all(feature = "defmt-format", target_os = "none"))]
use defmt;

/// Modbus exception codes as defined in the Modbus Application Protocol Specification V1.1b3.
///
/// These codes are used in exception responses (function code | 0x80) to indicate
/// the type of error that occurred when processing a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ExceptionCode {
    /// 0x01: Illegal Function - The function code is not supported by the server.
    IllegalFunction = 0x01,
    /// 0x02: Illegal Data Address - The addressed register does not exist.
    IllegalDataAddress = 0x02,
    /// 0x03: Illegal Data Value - The quantity of items to read/write is invalid.
    IllegalDataValue = 0x03,
    /// 0x04: Server Device Failure - Unrecoverable device failure.
    ServerDeviceFailure = 0x04,
    /// 0x0A: Gateway Path Unavailable - Specialized use in gateways.
    GatewayPathUnavailable = 0x0A,
    /// 0x0B: Gateway Target Device Failed to Respond - Specialized use in gateways.
    GatewayTargetDeviceFailedToRespond = 0x0B,
}

impl From<ExceptionCode> for u8 {
    fn from(code: ExceptionCode) -> Self {
        code as u8
    }
}

/// Represents a Modbus error.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MbusError {
    /// An error occurred while parsing the Modbus ADU.
    ParseError,
    /// This is used for receieved frame is fundamentally malformed
    BasicParseError,
    /// The transaction timed out waiting for a response.
    Timeout,
    /// The server responded with a Modbus exception code.
    ModbusException(u8),
    /// An I/O error occurred during TCP communication.
    IoError,
    /// An unexpected error occurred.
    Unexpected,
    /// The connection was lost during an active transaction.
    ConnectionLost,
    /// The function code is not supported
    UnsupportedFunction(u8),
    /// The sub-function code is not available
    ReservedSubFunction(u16),
    /// The PDU length is invalid
    InvalidPduLength,
    /// The ADU length is invalid
    InvalidAduLength,
    /// Connection failed
    ConnectionFailed,
    /// Connection closed
    ConnectionClosed,
    /// The data was too large for the buffer
    BufferTooSmall,
    /// Buffer length is not matching
    BufferLenMissmatch,
    /// Failed to send data
    SendFailed,
    /// Invalid address
    InvalidAddress,
    /// Invalid offset
    InvalidOffset,
    /// Too many requests in flight, expected responses buffer is full
    TooManyRequests,
    /// Invalid function code
    InvalidFunctionCode,
    /// No retries left for the transaction
    NoRetriesLeft,
    /// Too many sub-requests in a PDU, Max allowed is 35
    TooManyFileReadSubRequests,
    /// File read PDU overflow, total length of file read sub-requests exceeds maximum allowed bytes per PDU
    FileReadPduOverflow,
    /// An unexpected response was received that does not match the expected response type for the transaction.
    UnexpectedResponse,
    /// The transport is invalid for the requested operation
    InvalidTransport,
    /// Invalid slave address
    InvalidSlaveAddress,
    /// Checksum error
    ChecksumError,
    /// Invalid configuration
    InvalidConfiguration,
    /// Invalid number of expected responses.
    ///
    /// For Modbus Serial transports, only one request may be in flight at a time,
    /// so the expected-response queue size must be exactly `1`.
    InvalidNumOfExpectedRsps,
    /// Invalid data length
    InvalidDataLen,
    /// Invalid Quantity
    InvalidQuantity,
    /// Invalid Value
    InvalidValue,
    /// Invalid Masking value
    InvalidAndMask,
    /// Invalid Masking value
    InvalidOrMask,
    /// Invalid byte count
    InvalidByteCount,
    /// Invalid device identification
    InvalidDeviceIdentification,
    /// Invalid device id code
    InvalidDeviceIdCode,
    /// Invalid MEI type
    InvalidMeiType,
    /// Invalid broadcast address (0): Broadcast must be created explicitly.
    /// Use `UnitIdOrSlaveAddr::new_broadcast_address()` to signal broadcast intent.
    InvalidBroadcastAddress,
    /// Broadcast not allowed.
    ///
    /// Note: This variant name contains a historical typo and is kept for
    /// compatibility with existing code.
    BroadcastNotAllowed,
    /// Transport detected a protocol-level framing or timing violation.
    ///
    /// For serial transports this typically indicates an inter-character gap
    /// exceeding t1.5 character times within a frame.  The server must discard
    /// any partially accumulated frame data and resume listening for new frames.
    ///
    /// This is **not** a connection-level error — the bus remains usable.
    FramingError,
}

impl MbusError {
    /// Returns the canonical "broadcast not allowed" error.
    ///
    /// This helper exists to provide a correctly spelled API path while
    /// preserving the legacy enum variant name for compatibility.
    pub const fn broadcast_not_allowed() -> Self {
        Self::BroadcastNotAllowed
    }
}

#[cfg(all(feature = "defmt-format", target_os = "none"))]
impl defmt::Format for MbusError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            MbusError::ParseError => defmt::write!(
                f,
                "Parse error: An error occurred while parsing the Modbus ADU"
            ),
            MbusError::BasicParseError => defmt::write!(
                f,
                "Basic parse error: The received frame is fundamentally malformed"
            ),
            MbusError::Timeout => defmt::write!(
                f,
                "Timeout: The transaction timed out waiting for a response"
            ),
            MbusError::ModbusException(code) => defmt::write!(
                f,
                "Modbus exception: The server responded with exception code 0x{:02X}",
                code
            ),
            MbusError::IoError => defmt::write!(
                f,
                "I/O error: An I/O error occurred during TCP communication"
            ),
            MbusError::Unexpected => {
                defmt::write!(f, "Unexpected error: An unexpected error occurred")
            }
            MbusError::ConnectionLost => defmt::write!(
                f,
                "Connection lost: The connection was lost during an active transaction"
            ),
            MbusError::UnsupportedFunction(code) => defmt::write!(
                f,
                "Unsupported function: Function code 0x{:02X} is not supported",
                code
            ),
            MbusError::ReservedSubFunction(code) => defmt::write!(
                f,
                "Reserved sub-function: Sub-function code 0x{:04X} is not available",
                code
            ),
            MbusError::InvalidPduLength => {
                defmt::write!(f, "Invalid PDU length: The PDU length is invalid")
            }
            MbusError::InvalidAduLength => {
                defmt::write!(f, "Invalid ADU length: The ADU length is invalid")
            }
            MbusError::ConnectionFailed => defmt::write!(f, "Connection failed"),
            MbusError::ConnectionClosed => defmt::write!(f, "Connection closed"),
            MbusError::BufferTooSmall => {
                defmt::write!(f, "Buffer too small: The data was too large for the buffer")
            }
            MbusError::BufferLenMissmatch => {
                defmt::write!(f, "Buffer length mismatch: Buffer length is not matching")
            }
            MbusError::SendFailed => defmt::write!(f, "Send failed: Failed to send data"),
            MbusError::InvalidAddress => defmt::write!(f, "Invalid address"),
            MbusError::TooManyRequests => {
                defmt::write!(f, "Too many requests: Expected responses buffer is full")
            }
            MbusError::InvalidFunctionCode => defmt::write!(f, "Invalid function code"),
            MbusError::NoRetriesLeft => defmt::write!(f, "No retries left for the transaction"),
            MbusError::TooManyFileReadSubRequests => defmt::write!(
                f,
                "Too many sub-requests: Maximum of 35 sub-requests per PDU allowed"
            ),
            MbusError::FileReadPduOverflow => defmt::write!(
                f,
                "File read PDU overflow: Total length of file read sub-requests exceeds maximum allowed bytes per PDU"
            ),
            MbusError::UnexpectedResponse => defmt::write!(
                f,
                "Unexpected response: An unexpected response was received"
            ),
            MbusError::InvalidTransport => defmt::write!(
                f,
                "Invalid transport: The transport is invalid for the requested operation"
            ),
            MbusError::InvalidSlaveAddress => defmt::write!(
                f,
                "Invalid slave address: The provided slave address is invalid"
            ),
            MbusError::ChecksumError => defmt::write!(
                f,
                "Checksum error: The received frame has an invalid checksum"
            ),
            MbusError::InvalidConfiguration => defmt::write!(
                f,
                "Invalid configuration: The provided configuration is invalid"
            ),
            MbusError::InvalidNumOfExpectedRsps => defmt::write!(
                f,
                "Invalid number of expected responses: for serial transports the queue size N must be exactly 1"
            ),
            MbusError::InvalidDataLen => defmt::write!(
                f,
                "Invalid data length: The provided data length is invalid"
            ),
            MbusError::InvalidQuantity => {
                defmt::write!(f, "Invalid quantity: The provided quantity is invalid")
            }
            MbusError::InvalidValue => {
                defmt::write!(f, "Invalid value: The provided value is invalid")
            }
            MbusError::InvalidAndMask => {
                defmt::write!(f, "Invalid AND mask: The provided AND mask is invalid")
            }
            MbusError::InvalidOrMask => {
                defmt::write!(f, "Invalid OR mask: The provided OR mask is invalid")
            }
            MbusError::InvalidByteCount => {
                defmt::write!(f, "Invalid byte count: The provided byte count is invalid")
            }
            MbusError::InvalidDeviceIdentification => defmt::write!(
                f,
                "Invalid device identification: The provided device identification is invalid"
            ),
            MbusError::InvalidDeviceIdCode => defmt::write!(
                f,
                "Invalid device ID code: The provided device ID code is invalid"
            ),
            MbusError::InvalidMeiType => {
                defmt::write!(f, "Invalid MEI type: The provided MEI type is invalid")
            }
            MbusError::InvalidBroadcastAddress => defmt::write!(
                f,
                "Invalid broadcast address: The provided broadcast address (0) is invalid. Must use UnitIdOrSlaveAddr::new_broadcast_address() instead."
            ),
            MbusError::BroadcastNotAllowed => {
                defmt::write!(f, "Broadcast not allowed: Broadcast not allowed")
            }
            MbusError::InvalidOffset => {
                defmt::write!(f, "Invalid offset: The provided offset is invalid")
            }
            MbusError::FramingError => {
                defmt::write!(
                    f,
                    "Framing error: Transport detected a protocol timing violation"
                )
            }
        }
    }
}

#[cfg(feature = "error-trait")]
impl core::fmt::Display for MbusError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MbusError::ParseError => write!(f, "Parse error"),
            MbusError::BasicParseError => write!(f, "Basic parse error"),
            MbusError::Timeout => write!(f, "Timeout"),
            MbusError::ModbusException(code) => write!(f, "Modbus exception 0x{code:02X}"),
            MbusError::IoError => write!(f, "I/O error"),
            MbusError::Unexpected => write!(f, "Unexpected error"),
            MbusError::ConnectionLost => write!(f, "Connection lost"),
            MbusError::UnsupportedFunction(code) => write!(f, "Unsupported function 0x{code:02X}"),
            MbusError::ReservedSubFunction(code) => write!(f, "Reserved sub-function 0x{code:04X}"),
            MbusError::InvalidPduLength => write!(f, "Invalid PDU length"),
            MbusError::InvalidAduLength => write!(f, "Invalid ADU length"),
            MbusError::ConnectionFailed => write!(f, "Connection failed"),
            MbusError::ConnectionClosed => write!(f, "Connection closed"),
            MbusError::BufferTooSmall => write!(f, "Buffer too small"),
            MbusError::BufferLenMissmatch => write!(f, "Buffer length mismatch"),
            MbusError::SendFailed => write!(f, "Send failed"),
            MbusError::InvalidAddress => write!(f, "Invalid address"),
            MbusError::InvalidOffset => write!(f, "Invalid offset"),
            MbusError::TooManyRequests => write!(f, "Too many requests"),
            MbusError::InvalidFunctionCode => write!(f, "Invalid function code"),
            MbusError::NoRetriesLeft => write!(f, "No retries left"),
            MbusError::TooManyFileReadSubRequests => write!(f, "Too many file read sub-requests"),
            MbusError::FileReadPduOverflow => write!(f, "File read PDU overflow"),
            MbusError::UnexpectedResponse => write!(f, "Unexpected response"),
            MbusError::InvalidTransport => write!(f, "Invalid transport"),
            MbusError::InvalidSlaveAddress => write!(f, "Invalid slave address"),
            MbusError::ChecksumError => write!(f, "Checksum error"),
            MbusError::InvalidConfiguration => write!(f, "Invalid configuration"),
            MbusError::InvalidNumOfExpectedRsps => {
                write!(f, "Invalid number of expected responses")
            }
            MbusError::InvalidDataLen => write!(f, "Invalid data length"),
            MbusError::InvalidQuantity => write!(f, "Invalid quantity"),
            MbusError::InvalidValue => write!(f, "Invalid value"),
            MbusError::InvalidAndMask => write!(f, "Invalid AND mask"),
            MbusError::InvalidOrMask => write!(f, "Invalid OR mask"),
            MbusError::InvalidByteCount => write!(f, "Invalid byte count"),
            MbusError::InvalidDeviceIdentification => write!(f, "Invalid device identification"),
            MbusError::InvalidDeviceIdCode => write!(f, "Invalid device ID code"),
            MbusError::InvalidMeiType => write!(f, "Invalid MEI type"),
            MbusError::InvalidBroadcastAddress => write!(f, "Invalid broadcast address"),
            MbusError::BroadcastNotAllowed => write!(f, "Broadcast not allowed"),
            MbusError::FramingError => write!(f, "Framing error"),
        }
    }
}

#[cfg(feature = "error-trait")]
impl core::error::Error for MbusError {}
