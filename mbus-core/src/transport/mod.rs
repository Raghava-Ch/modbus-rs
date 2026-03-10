pub mod checksum;
use core::str::FromStr;

use crate::{data_unit::common::SlaveAddress, errors::MbusError};
use heapless::{String, Vec};

/// The default TCP port for Modbus communication.
const MODBUS_TCP_DEFAULT_PORT: u16 = 502;

/// Top-level configuration for Modbus communication, supporting different transport layers.
#[derive(Debug)]
pub enum ModbusConfig {
    /// Configuration for Modbus TCP/IP.
    Tcp(ModbusTcpConfig),
    /// Configuration for Modbus Serial (RTU or ASCII).
    Serial(ModbusSerialConfig),
}

/// Parity bit configuration for serial communication.
#[derive(Debug, Default)]
pub enum Parity {
    /// No parity bit is used.
    None,
    /// Even parity: the number of 1-bits in the data plus parity bit is even.
    #[default]
    Even,
    /// Odd parity: the number of 1-bits in the data plus parity bit is odd.
    Odd,
}

/// Configuration parameters for establishing a Modbus Serial connection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SerialMode {
    /// Modbus RTU mode, which uses binary encoding and CRC error checking.
    #[default]
    Rtu,
    /// Modbus ASCII mode, which uses ASCII encoding and LRC error checking.
    Ascii,
}

/// Baud rate configuration for serial communication.
#[derive(Debug, Default)]
pub enum BaudRate {
    /// Standard baud rate of 9600 bits per second.
    Baud9600,
    /// Standard baud rate of 19200 bits per second.
    #[default]
    Baud19200,
    /// Custom baud rate.
    Custom(u32), // Allow custom baud rates for flexibility
}

#[derive(Debug)]
/// Configuration parameters for establishing a Modbus Serial connection.
pub struct ModbusSerialConfig {
    /// The path to the serial port (e.g., "/dev/ttyUSB0" or "COM1").
    pub port_path: heapless::String<64>,
    /// The serial mode to use (RTU or ASCII).
    pub mode: SerialMode,
    /// Communication speed in bits per second (e.g., 9600, 115200).
    pub baud_rate: BaudRate,
    /// Number of data bits per character (typically 8 for RTU, 7 for ASCII).
    pub data_bits: u8,
    /// Number of stop bits (This will be recalculated before calling the transport layer).
    pub stop_bits: u8,
    /// The parity checking mode.
    pub parity: Parity,
    /// Timeout for waiting for a response in milliseconds.
    pub response_timeout_ms: u32,
}

#[derive(Debug)]
/// Configuration parameters for establishing a Modbus TCP connection.
pub struct ModbusTcpConfig {
    /// The hostname or IP address of the Modbus TCP server to connect to.
    pub host: heapless::String<64>, // Increased capacity for host string to accommodate longer IP addresses/hostnames
    /// The TCP port number on which the Modbus server is listening (default is typically 502).
    pub port: u16,

    // Optional parameters for connection management (can be set to default values if not needed)
    pub connection_timeout_ms: u32, // Timeout for establishing a connection in milliseconds
    pub response_timeout_ms: u32,   // Timeout for waiting for a response in milliseconds

    pub retry_attempts: u8, // Number of retry attempts for failed operations
    pub keep_alive_interval_ms: u32, // Interval for sending keep-alive messages in milliseconds
}

/// The transport module defines the `Transport` trait and related types for managing Modbus TCP communication.
impl ModbusTcpConfig {
    /// Creates a new `ModbusTcpConfig` instance with the specified host and port.
    /// # Arguments
    /// * `host` - The hostname or IP address of the Modbus TCP server to connect to.
    /// * `port` - The TCP port number on which the Modbus server is listening.
    /// # Returns
    /// A new `ModbusTcpConfig` instance with the provided host and port.
    pub fn default(host: &str) -> Result<Self, MbusError> {
        let host_string = String::from_str(host).map_err(|_| MbusError::BufferTooSmall)?; // Return error if host string is too long
        Ok(Self {
            host: host_string,
            port: MODBUS_TCP_DEFAULT_PORT,
            connection_timeout_ms: 5000,
            response_timeout_ms: 5000,
            retry_attempts: 3,
            keep_alive_interval_ms: 30000,
        })
    }

    /// Creates a new `ModbusTcpConfig` instance with the specified host and port.
    /// # Arguments
    /// * `host` - The hostname or IP address of the Modbus TCP server to connect to.
    /// * `port` - The TCP port number on which the Modbus server is listening.
    /// # Returns
    /// A new `ModbusTcpConfig` instance with the provided host and port.
    pub fn new(host: &str, port: u16) -> Result<Self, MbusError> {
        let host_string = String::from_str(host).map_err(|_| MbusError::BufferTooSmall)?; // Return error if host string is too long
        Ok(Self {
            host: host_string,
            port,
            connection_timeout_ms: 5000,
            response_timeout_ms: 5000,
            retry_attempts: 3,
            keep_alive_interval_ms: 30000,
        })
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
    /// Invalid configuration
    InvalidConfiguration
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
            TransportError::InvalidConfiguration => write!(f, "Invalid configuration")
        }
    }
}

/// Implements the core standard `Error` trait for `TransportError`, allowing it to be used with Rust's error handling ecosystem.
impl core::error::Error for TransportError {}

/// An enumeration to specify the type of transport to use.
#[derive(Debug, PartialEq, Eq)]
pub enum TransportType {
    /// Standard library TCP transport implementation.
    StdTcp,
    /// Standard library Serial transport implementation.
    StdSerial(SlaveAddress, SerialMode),
    /// Custom TCP transport implementation.
    CustomTcp,
    /// Custom Serial transport implementation.
    CustomSerial(SlaveAddress, SerialMode),
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
            TransportError::InvalidConfiguration => MbusError::InvalidConfiguration,
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
    /// * `config` - The `ModbusTcpConfig` containing the host and port of the Modbus TCP server.
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error>;

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

pub trait TimeKeeper {
    // A simple mock for current_millis for no_std compatibility in tests.
    // In a real no_std environment, this would come from a hardware timer.
    fn current_millis(&self) -> u64;
}
