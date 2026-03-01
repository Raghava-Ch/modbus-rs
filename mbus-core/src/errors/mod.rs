
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MbusError {
    /// An error occurred while parsing the Modbus ADU.
    ParseError,
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
}