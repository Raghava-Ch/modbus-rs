use heapless::Vec;
use crate::errors::MbusError;

/// Represents errors that can occur at the Modbus TCP transport layer.
#[derive(Debug, PartialEq, Eq)]
pub enum ModbusTcpTransportError {
    /// The connection attempt failed.
    ConnectionFailed,
    /// The connection was unexpectedly closed.
    ConnectionClosed,
    /// An I/O error occurred during send or receive.
    IoError,
    /// A timeout occurred during a network operation.
    Timeout,
    /// The received data was too large for the buffer.
    BufferTooSmall,
    /// An unexpected error occurred.
    Unexpected,
    // Add more specific errors as needed
}

impl From<ModbusTcpTransportError> for MbusError {
    fn from(err: ModbusTcpTransportError) -> Self {
        match err {
            ModbusTcpTransportError::ConnectionFailed => MbusError::ConnectionFailed,
            ModbusTcpTransportError::ConnectionClosed => MbusError::ConnectionClosed,
            ModbusTcpTransportError::IoError => MbusError::IoError,
            ModbusTcpTransportError::Timeout => MbusError::Timeout,
            ModbusTcpTransportError::BufferTooSmall => MbusError::BufferTooSmall,
            ModbusTcpTransportError::Unexpected => MbusError::Unexpected,
        }
    }
}

/// A trait defining the interface for a Modbus TCP transport layer.
///
/// Implementors of this trait are responsible for managing the underlying
/// TCP connection, sending and receiving raw Modbus ADU bytes.
pub trait ModbusTcpTransport {
    /// The error type specific to this transport implementation.
    type Error: Into<MbusError>;

    /// Establishes a TCP connection to the specified remote address.
    ///
    /// # Arguments
    /// * `addr` - The address of the Modbus TCP server (e.g., "192.168.1.1:502").
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, addr: &str) -> Result<(), Self::Error>;

    /// Closes the active TCP connection.
    fn disconnect(&mut self) -> Result<(), Self::Error>;

    /// Sends a Modbus Application Data Unit (ADU) over the TCP connection.
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error>;

    /// Receives a Modbus Application Data Unit (ADU) from the TCP connection.
    fn recv(&mut self) -> Result<Vec<u8, 260>, Self::Error>;

    /// Checks if the transport is currently connected to a remote host.
    fn is_connected(&self) -> bool;
}