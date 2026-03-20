//! Modbus Transport Layer Module
//!
//! This module defines the abstractions and configurations required for different
//! Modbus transport protocols, including TCP/IP and Serial (RTU/ASCII).
//!
//! It provides:
//! - The [`Transport`] trait: A common interface for sending and receiving Modbus ADUs.
//! - Configuration structures ([`ModbusTcpConfig`], [`ModbusSerialConfig`]): Parameters
//!   for establishing connections.
//! - Error handling: [`TransportError`] for mapping low-level I/O issues to Modbus-specific contexts.
//! - Checksum utilities: CRC16 and LRC via the [`checksum`] submodule.

pub mod checksum;
use core::str::FromStr;

use crate::{data_unit::common::MAX_ADU_FRAME_LEN, errors::MbusError};
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

impl ModbusConfig {
    pub fn retry_attempts(&self) -> u8 {
        match self {
            ModbusConfig::Tcp(config) => config.retry_attempts,
            ModbusConfig::Serial(config) => config.retry_attempts,
        }
    }
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
pub struct ModbusSerialConfig<const PORT_PATH_LEN: usize = 64> {
    /// The path to the serial port (e.g., "/dev/ttyUSB0" or "COM1").
    pub port_path: heapless::String<PORT_PATH_LEN>,
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
    /// Number of retries for failed operations.
    pub retry_attempts: u8,
}

#[derive(Debug)]
/// Configuration parameters for establishing a Modbus TCP connection.
pub struct ModbusTcpConfig {
    /// The hostname or IP address of the Modbus TCP server to connect to.
    pub host: heapless::String<64>, // Increased capacity for host string to accommodate longer IP addresses/hostnames
    /// The TCP port number on which the Modbus server is listening (default is typically 502).
    pub port: u16,

    // Optional parameters for connection management (can be set to default values if not needed)
    /// Timeout for establishing a connection in milliseconds
    pub connection_timeout_ms: u32,
    /// Timeout for waiting for a response in milliseconds
    pub response_timeout_ms: u32,
    /// Number of retry attempts for failed operations
    pub retry_attempts: u8,
    /// Interval for sending keep-alive messages in milliseconds
    /// This value is only applicable for `StdTcp` transport type
    /// The default `mbus-tcp` crate does not support this feature. It is intended for custom `Transport` trait implementations
    /// where keep-alive functionality might be desired.
    pub keep_alive_interval_ms: u32,
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
    InvalidConfiguration,
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
            TransportError::InvalidConfiguration => write!(f, "Invalid configuration"),
        }
    }
}

/// Implements the core standard `Error` trait for `TransportError`, allowing it to be used with Rust's error handling ecosystem.
impl core::error::Error for TransportError {}

/// An enumeration to specify the type of transport to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportType {
    /// Standard library TCP transport implementation.
    StdTcp,
    /// Standard library Serial transport implementation.
    StdSerial(SerialMode),
    /// Custom TCP transport implementation.
    CustomTcp,
    /// Custom Serial transport implementation.
    CustomSerial(SerialMode),
}

impl TransportType {
    pub fn is_tcp_type(&self) -> bool {
        match self {
            TransportType::StdTcp | TransportType::CustomTcp => true,
            _ => false,
        }
    }

    pub fn is_serial_type(&self) -> bool {
        match self {
            TransportType::StdSerial(_) | TransportType::CustomSerial(_) => true,
            _ => false,
        }
    }
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

/// A wrapper type representing either a Modbus TCP Unit Identifier or a Serial Slave Address.
///
/// In Modbus TCP, this is the Unit ID (typically 1 byte).
/// In Modbus RTU/ASCII, this is the Slave Address (1-247).
/// 1 to 247: These addresses are used for individual slave devices. Each device on the network must have a unique address within this range.
/// 0: This address is reserved for broadcast messages, meaning a request sent with Unit ID 0 will be processed by all slave devices, but no response is returned.
/// 248 to 255: These addresses are reserved and should not be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitIdOrSlaveAddr(u8);

impl UnitIdOrSlaveAddr {
    /// Creates a new `SlaveAddress` instance.
    pub fn new(address: u8) -> Result<Self, MbusError> {
        if (1..=247).contains(&address) {
            return Ok(Self(address));
        }

        if 0 == address {
            return Err(MbusError::InvalidSlaveAddress);
        }
        Err(MbusError::InvalidSlaveAddress)
    }

    pub fn new_broadcast_address() -> Self {
        Self(0)
    }

    /// Returns `true` if the address is the Modbus broadcast address (0).
    pub fn is_broadcast(&self) -> bool {
        self.0 == 0
    }

    /// Returns the raw `u8` value of the slave address.
    ///
    /// # Returns
    /// The `u8` value representing the slave address.
    pub fn get(&self) -> u8 {
        self.0
    }

    /// Provides a default value for initialization or error states.
    ///
    /// # Warning
    /// This returns `255`, which is outside the valid Modbus slave address range (1-247).
    /// It is intended to be used as a sentinel value to represent an uninitialized or
    /// invalid address state that must be handled by the application logic. This value should not be sent over the wire.
    pub fn default() -> Self {
        // 255 is in the reserved range (248-255) and serves as a safe
        // "Null" or "Error" marker in this context.
        Self(255)
    }
}

pub trait UidSaddrFrom {
    fn from_u8(uid_saddr: u8) -> Self;
}

impl UidSaddrFrom for UnitIdOrSlaveAddr {
    fn from_u8(value: u8) -> Self {
        UnitIdOrSlaveAddr::new(value).unwrap_or(Self::default())
    }
}

impl Into<u8> for UnitIdOrSlaveAddr {
    fn into(self) -> u8 {
        self.get()
    }
}

/// Implementation of `TryFrom` to allow safe conversion from a raw `u8`
/// to a validated `UnitIdOrSlaveAddr`.
impl TryFrom<u8> for UnitIdOrSlaveAddr {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        // Delegates to the new() constructor which performs range validation (1-247)
        UnitIdOrSlaveAddr::new(value)
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
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error>;

    /// Checks if the transport is currently connected to a remote host.
    fn is_connected(&self) -> bool;

    /// Returns the type of transport being used (e.g., TCP, Serial).
    fn transport_type(&self) -> TransportType;
}

/// A trait for abstracting time-related operations, primarily for mocking in tests
/// and providing a consistent interface for `no_std` environments.
pub trait TimeKeeper {
    /// A simple mock for current_millis for no_std compatibility in tests.
    /// In a real no_std environment, this would come from a hardware timer.
    /// Returns the current time in milliseconds.
    ///
    /// # Returns
    /// The current time in milliseconds.
    fn current_millis(&self) -> u64;
}
