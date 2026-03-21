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

/// Implements common functionality for `ModbusConfig`.
impl ModbusConfig {
    /// Returns the number of retry attempts configured for the transport.
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
    /// Returns `true` if the transport type is TCP (StdTcp or CustomTcp).
    pub fn is_tcp_type(&self) -> bool {
        match self {
            TransportType::StdTcp | TransportType::CustomTcp => true,
            _ => false,
        }
    }

    /// Returns `true` if the transport type is serial (RTU or ASCII).
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

/// A type-safe wrapper for Modbus Unit Identifiers (TCP) and Slave Addresses (Serial).
/// 
/// ### Address Ranges:
/// - **1 to 247**: Valid Unicast addresses for individual slave devices.
/// - **0**: Reserved for **BROADCAST** operations.
/// - **248 to 255**: Reserved/Invalid addresses.
/// 
/// ### ⚠️ Important: Broadcasting (Address 0)
/// To prevent accidental broadcast requests (which are processed by all devices and 
/// **never** return a response), address `0` cannot be passed to the standard `new()` 
/// or `try_from()` constructors.
/// 
/// Developers **must** explicitly use [`UnitIdOrSlaveAddr::new_broadcast_address()`] 
/// to signal intent for a broadcast operation.
/// 
/// *Note: Broadcasts are generally only supported for Write operations on Serial transports.*
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnitIdOrSlaveAddr(u8);

impl UnitIdOrSlaveAddr {
    /// Creates a new `UnitIdOrSlaveAddr` instance with the specified address.
    ///
    /// ### Address Ranges:
    /// - **1 to 247**: Valid Unicast addresses for individual slave devices.
    /// - **0**: Reserved for **BROADCAST** operations.
    /// - **248 to 255**: Reserved/Invalid addresses.
    /// 
    /// ### ⚠️ Important: Broadcasting (Address 0)
    /// To prevent accidental broadcast requests (which are processed by all devices and 
    /// **never** return a response), address `0` cannot be passed to the standard `new()` 
    /// or `try_from()` constructors.
    /// 
    /// Developers **must** explicitly use [`UnitIdOrSlaveAddr::new_broadcast_address()`] 
    /// to signal intent for a broadcast operation.
    /// 
    /// # Arguments:
    /// - `address`: The `u8` value representing the Unit ID or Slave Address.
    pub fn new(address: u8) -> Result<Self, MbusError> {
        if (1..=247).contains(&address) {
            return Ok(Self(address));
        }

        if 0 == address {
            return Err(MbusError::InvalidSlaveAddress);
        }
        Err(MbusError::InvalidSlaveAddress)
    }

    /// Creates a new `UnitIdOrSlaveAddr` instance representing the broadcast address (`0`).
    /// 
    /// *Note: Broadcasts are generally only supported for Write operations on Serial transports.*
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
    /// # ⚠️ Warning
    /// This returns `255`, which is outside the valid Modbus slave address range (1-247).
    /// It is intended to be used as a sentinel value to represent an uninitialized or
    /// invalid address state that must be handled by the application logic. 
    /// This value will/should not be sent over the wire.
    pub fn default() -> Self {
        // 255 is in the reserved range (248-255) and serves as a safe
        // "Null" or "Error" marker in this context.
        Self(255)
    }
}

/// A trait for types that can be created from a `u8` Unit ID or Slave Address.
pub trait UidSaddrFrom {
    /// Creates a new instance from a `u8` Unit ID or Slave Address.
    fn from_u8(uid_saddr: u8) -> Self;
}

/// Implementation of `UidSaddrFrom` for `UnitIdOrSlaveAddr`.
impl UidSaddrFrom for UnitIdOrSlaveAddr {
    /// Creates a new instance from a `u8` Unit ID or Slave Address.
    ///
    /// # Arguments
    /// * `value` - The `u8` value representing the Unit ID or Slave Address.
    ///
    /// # Returns
    /// A new `UnitIdOrSlaveAddr` instance.
    fn from_u8(value: u8) -> Self {
        UnitIdOrSlaveAddr::new(value).unwrap_or(Self::default())
    }
}

impl Into<u8> for UnitIdOrSlaveAddr {
    /// Implementation of `Into<u8>` for `UnitIdOrSlaveAddr`.
    ///
    /// This allows `UnitIdOrSlaveAddr` to be converted into a `u8` value.
    ///
    /// # Returns
    /// The raw `u8` value of the `UnitIdOrSlaveAddr`.
    ///
    fn into(self) -> u8 {
        self.get()
    }
}

/// Implementation of `TryFrom` to allow safe conversion from a raw `u8`
/// to a validated `UnitIdOrSlaveAddr`.
impl TryFrom<u8> for UnitIdOrSlaveAddr {
    type Error = MbusError;
    /// Attempts to create a new `UnitIdOrSlaveAddr` from a raw `u8` value.
    /// 
    fn try_from(value: u8) -> Result<Self, Self::Error> {
        // Delegates to the new() constructor which performs range validation (1-247)
        UnitIdOrSlaveAddr::new(value)
    }
}

/// A unified trait defining the interface for any Modbus physical or network transport layer.
///
/// This trait abstracts the underlying communication medium (e.g., TCP socket, Serial COM port,
/// or a mocked in-memory buffer) so that the higher-level Modbus Client Services can orchestrate
/// transactions without needing to know the specifics of the hardware layer.
///
/// # Implementor Responsibilities
/// Implementors of this trait must ensure:
/// - **Connection Management**: Handling the initialization and teardown of the physical link.
/// - **Framing**: Reading exactly one complete Modbus Application Data Unit (ADU) at a time for TCP.
///   For TCP, this means parsing the MBAP header to determine the length. For Serial (RTU), this
///   involves managing inter-frame timing silences or LRC/CRCs. In other words, just provide the available bytes;
///   the protocol stack is intelligent enough to construct the full frame. If a timeout occurs, the stack will clear the buffer.
pub trait Transport {
    /// The specific error type returned by this transport implementation.
    /// It must be convertible into the common `MbusError` for upper-layer processing.
    type Error: Into<MbusError>;

    /// Establishes the physical or logical connection to the Modbus server/slave.
    ///
    /// # Arguments
    /// * `config` - A generalized `ModbusConfig` enum containing specific settings (like
    ///   IP/Port for TCP or Baud Rate/Parity for Serial connections).
    ///
    /// # Returns
    /// - `Ok(())` if the underlying port was opened or socket successfully connected.
    /// - `Err(Self::Error)` if the initialization fails (e.g., port busy, network unreachable).
    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error>;

    /// Gracefully closes the active connection and releases underlying resources.
    /// 
    /// After calling this method, subsequent calls to `send` or `recv` should fail until
    /// `connect` is called again.
    fn disconnect(&mut self) -> Result<(), Self::Error>;

    /// Transmits a complete Modbus Application Data Unit (ADU) over the transport medium.
    ///
    /// The provided `adu` slice contains the fully formed byte frame, including all headers
    /// (like MBAP for TCP) and footers (like CRC/LRC for Serial).
    ///
    /// # Arguments
    /// * `adu` - A contiguous byte slice representing the packet to send.
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error>;

    /// Receives available bytes from the transport medium in a **non-blocking** manner.
    ///
    /// # Implementation Details
    /// - **TCP**: Implementors should ideally return a complete Modbus Application Data Unit (ADU).
    /// - **Serial**: Implementors can return any number of available bytes. The protocol stack
    ///   is responsible for accumulating these fragments into a complete frame.
    /// - **Timeouts**: If the protocol stack fails to assemble a full frame within the configured
    ///   `response_timeout_ms`, it will automatically clear its internal buffers.
    ///
    /// # Returns
    /// - `Ok(Vec<u8, MAX_ADU_FRAME_LEN>)`: A heapless vector containing the bytes read since the last call.
    /// - `Err(Self::Error)`: Returns `TransportError::Timeout` if no data is currently available,
    ///   or other errors if the connection is lost or hardware fails.
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error>;

    /// Checks if the transport considers itself currently active and connected.
    ///
    /// Note: For connectionless or semi-connected states (like some RS-485 setups), this
    /// might continually return `true` as long as the local port is open.
    fn is_connected(&self) -> bool;

    /// Returns an identifier indicating the mode and type of this transport.
    ///
    /// The Modbus Client Services use this to determine how to strip network headers (like MBAP)
    /// or validate checksums based on whether it is a TCP, RTU, or ASCII implementation.
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
