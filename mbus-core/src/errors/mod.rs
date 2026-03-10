use core::fmt;

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
    InvalidConfiguration
}

impl fmt::Display for MbusError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            MbusError::ParseError => write!(
                f,
                "Parse error: An error occurred while parsing the Modbus ADU"
            ),
            MbusError::BasicParseError => write!(
                f,
                "Basic parse error: The received frame is fundamentally malformed"
            ),
            MbusError::Timeout => write!(
                f,
                "Timeout: The transaction timed out waiting for a response"
            ),
            MbusError::ModbusException(code) => write!(
                f,
                "Modbus exception: The server responded with exception code 0x{:02X}",
                code
            ),
            MbusError::IoError => write!(
                f,
                "I/O error: An I/O error occurred during TCP communication"
            ),
            MbusError::Unexpected => write!(f, "Unexpected error: An unexpected error occurred"),
            MbusError::ConnectionLost => write!(
                f,
                "Connection lost: The connection was lost during an active transaction"
            ),
            MbusError::UnsupportedFunction(code) => write!(
                f,
                "Unsupported function: Function code 0x{:02X} is not supported",
                code
            ),
            MbusError::ReservedSubFunction(code) => write!(
                f,
                "Reserved sub-function: Sub-function code 0x{:04X} is not available",
                code
            ),
            MbusError::InvalidPduLength => {
                write!(f, "Invalid PDU length: The PDU length is invalid")
            }
            MbusError::InvalidAduLength => {
                write!(f, "Invalid ADU length: The ADU length is invalid")
            }
            MbusError::ConnectionFailed => write!(f, "Connection failed"),
            MbusError::ConnectionClosed => write!(f, "Connection closed"),
            MbusError::BufferTooSmall => {
                write!(f, "Buffer too small: The data was too large for the buffer")
            }
            MbusError::BufferLenMissmatch => {
                write!(f, "Buffer length mismatch: Buffer length is not matching")
            }
            MbusError::SendFailed => write!(f, "Send failed: Failed to send data"),
            MbusError::InvalidAddress => write!(f, "Invalid address"),
            MbusError::TooManyRequests => {
                write!(f, "Too many requests: Expected responses buffer is full")
            }
            MbusError::InvalidFunctionCode => write!(f, "Invalid function code"),
            MbusError::NoRetriesLeft => write!(f, "No retries left for the transaction"),
            MbusError::TooManyFileReadSubRequests => write!(
                f,
                "Too many sub-requests: Maximum of 35 sub-requests per PDU allowed"
            ),
            MbusError::FileReadPduOverflow => write!(
                f,
                "File read PDU overflow: Total length of file read sub-requests exceeds maximum allowed bytes per PDU"
            ),
            MbusError::UnexpectedResponse => write!(
                f,
                "Unexpected response: An unexpected response was received"
            ),
            MbusError::InvalidTransport => write!(
                f,
                "Invalid transport: The transport is invalid for the requested operation"
            ),
            MbusError::InvalidSlaveAddress => write!(
                f,
                "Invalid slave address: The provided slave address is invalid"
            ),
            MbusError::ChecksumError => write!(
                f,
                "Checksum error: The received frame has an invalid checksum"
            ),
            MbusError::InvalidConfiguration => write!(
                f,
                "Invalid configuration: The provided configuration is invalid"
            ),
        }
    }
}

impl core::error::Error for MbusError {}
