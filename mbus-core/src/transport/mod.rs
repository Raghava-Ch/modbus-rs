
use core::str::FromStr;

use heapless::{String, Vec};
use crate::{errors::MbusError};

pub struct ModbusTcpConfig {
    pub host: heapless::String<16>,
    pub port: u16,
}

impl ModbusTcpConfig {
    pub fn new(host: &str, port: u16) -> Self {
        Self {
            host: String::from_str(host).unwrap_or_else(|_| String::new()), // Convert &str to heapless String, fallback to empty String on error
            port,
        }
    }
}

use core::fmt;

/// Represents errors that can occur at the Modbus TCP transport layer.
#[derive(Debug, PartialEq, Eq)]
pub enum TransportError {
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

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TransportError::ConnectionFailed => write!(f, "Connection failed"),
            TransportError::ConnectionClosed => write!(f, "Connection closed"),
            TransportError::IoError => write!(f, "I/O error"),
            TransportError::Timeout => write!(f, "Timeout"),
            TransportError::BufferTooSmall => write!(f, "Buffer too small"),
            TransportError::Unexpected => write!(f, "An unexpected error occurred"),
        }
    }
}

impl core::error::Error for TransportError {}

/// An enumeration to specify the type of transport to use.
#[derive(Debug, PartialEq, Eq)]
pub enum TransportType {
    /// Standard library TCP transport implementation.
    StdTcp,
    /// Standard library Serial transport implementation.
    StdSerial,
    /// Custom TCP transport implementation.
    CustomTcp,
    /// Custom Serial transport implementation.
    CustomSerial,
}


impl From<TransportError> for MbusError {
    fn from(err: TransportError) -> Self {
        match err {
            TransportError::ConnectionFailed => MbusError::ConnectionFailed,
            TransportError::ConnectionClosed => MbusError::ConnectionClosed,
            TransportError::IoError => MbusError::IoError,
            TransportError::Timeout => MbusError::Timeout,
            TransportError::BufferTooSmall => MbusError::BufferTooSmall,
            TransportError::Unexpected => MbusError::Unexpected,
        }
    }
}

/// A trait defining the interface for a Modbus TCP transport layer.
///
/// Implementors of this trait are responsible for managing the underlying
/// TCP connection, sending and receiving raw Modbus ADU bytes.
pub trait Transport {
    /// The error type specific to this transport implementation.
    type Error: Into<MbusError>;

    /// Establishes a TCP connection to the specified remote address.
    ///
    /// # Arguments
    /// * `addr` - The address of the Modbus TCP server (e.g., "192.168.1.1:502").
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, config: &ModbusTcpConfig) -> Result<(), Self::Error>;

    /// Closes the active TCP connection.
    fn disconnect(&mut self) -> Result<(), Self::Error>;

    /// Sends a Modbus Application Data Unit (ADU) over the TCP connection.
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error>;

    /// Receives a Modbus Application Data Unit (ADU) from the TCP connection.
    fn recv(&mut self) -> Result<Vec<u8, 260>, Self::Error>;

    /// Checks if the transport is currently connected to a remote host.
    fn is_connected(&self) -> bool;

    /// Returns the type of transport being used (e.g., TCP, Serial).
    fn transport_type(&self) -> TransportType;
}