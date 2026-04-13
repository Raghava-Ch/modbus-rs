//! # Modbus Transport Layer
//!
//! This module defines the abstractions and configurations required for transmitting
//! Modbus Application Data Units (ADUs) over various physical and logical mediums.
//!
//! ## Core Concepts
//! - **[`Transport`]**: A unified trait that abstracts the underlying communication
//!   (TCP, Serial, or Mock) from the high-level protocol logic.
//! - **[`ModbusConfig`]**: A comprehensive configuration enum for setting up
//!   TCP/IP or Serial (RTU/ASCII) parameters.
//! - **[`BackoffStrategy`]**: Poll-driven retry scheduling strategy used after timeouts.
//! - **[`JitterStrategy`]**: Optional jitter added on top of retry backoff delays.
//! - **[`RetryRandomFn`]**: Application-supplied random callback used only when jitter is enabled.
//! - **[`UnitIdOrSlaveAddr`]**: A type-safe wrapper ensuring that Modbus addresses
//!   stay within the valid range (1-247) and handling broadcast (0) explicitly.
//!
//! ## Design Goals
//! - **`no_std` Compatibility**: Uses `heapless` data structures and `core` traits
//!   to ensure the library can run on bare-metal embedded systems.
//! - **Non-blocking I/O**: The `Transport::recv` interface is designed to be polled,
//!   allowing the client to remain responsive without requiring an OS-level thread.
//! - **Scheduled retries**: Retry backoff/jitter values are consumed by higher layers
//!   to schedule retransmissions using timestamps, never by sleeping.
//! - **Extensibility**: Users can implement the `Transport` trait to support
//!   custom hardware (e.g., specialized UART drivers or proprietary TCP stacks).
//!
//! ## Error Handling
//! Errors are categorized into [`TransportError`], which can be seamlessly converted
//! into the top-level [`MbusError`] used throughout the crate.

pub mod checksum;
use core::str::FromStr;

use crate::{data_unit::common::MAX_ADU_FRAME_LEN, errors::MbusError};
use heapless::{String, Vec};

/// The default TCP port for Modbus communication.
const MODBUS_TCP_DEFAULT_PORT: u16 = 502;

/// Application-provided callback used to generate randomness for retry jitter.
///
/// The callback returns a raw `u32` value that is consumed by jitter logic.
/// The distribution does not need to be cryptographically secure. A simple
/// pseudo-random source from the target platform is sufficient.
pub type RetryRandomFn = fn() -> u32;

/// Retry delay strategy used after a request times out.
///
/// The delay is computed per retry attempt in a poll-driven manner. No internal
/// sleeping or blocking waits are performed by the library.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackoffStrategy {
    /// Retry immediately after timeout detection.
    Immediate,
    /// Retry using a constant delay in milliseconds.
    Fixed {
        /// Delay applied before each retry.
        delay_ms: u32,
    },
    /// Retry with an exponential sequence: `base_delay_ms * 2^(attempt-1)`.
    Exponential {
        /// Base delay for the first retry attempt.
        base_delay_ms: u32,
        /// Upper bound used to clamp growth.
        max_delay_ms: u32,
    },
    /// Retry with a linear sequence: `initial_delay_ms + (attempt-1) * increment_ms`.
    Linear {
        /// Delay for the first retry attempt.
        initial_delay_ms: u32,
        /// Increment added on every subsequent retry.
        increment_ms: u32,
        /// Upper bound used to clamp growth.
        max_delay_ms: u32,
    },
}

impl Default for BackoffStrategy {
    fn default() -> Self {
        Self::Immediate
    }
}

impl BackoffStrategy {
    /// Computes the base retry delay in milliseconds for a 1-based retry attempt index.
    ///
    /// `retry_attempt` is expected to start at `1` for the first retry after the
    /// initial request timeout.
    pub fn delay_ms_for_retry(&self, retry_attempt: u8) -> u32 {
        let attempt = retry_attempt.max(1);
        match self {
            BackoffStrategy::Immediate => 0,
            BackoffStrategy::Fixed { delay_ms } => *delay_ms,
            BackoffStrategy::Exponential {
                base_delay_ms,
                max_delay_ms,
            } => {
                let shift = (attempt.saturating_sub(1)).min(31);
                let factor = 1u32 << shift;
                base_delay_ms.saturating_mul(factor).min(*max_delay_ms)
            }
            BackoffStrategy::Linear {
                initial_delay_ms,
                increment_ms,
                max_delay_ms,
            } => {
                let growth = increment_ms.saturating_mul((attempt.saturating_sub(1)) as u32);
                initial_delay_ms.saturating_add(growth).min(*max_delay_ms)
            }
        }
    }
}

/// Jitter strategy applied on top of computed backoff delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JitterStrategy {
    /// Do not apply jitter.
    #[default]
    None,
    /// Apply symmetric percentage jitter around the base delay.
    ///
    /// For example, with `percent = 20` and base `100ms`, the final delay is
    /// in the range `[80ms, 120ms]`.
    Percentage {
        /// Maximum percentage variation from the base delay.
        percent: u8,
    },
    /// Apply symmetric bounded jitter in milliseconds around the base delay.
    ///
    /// For example, with `max_jitter_ms = 15` and base `100ms`, the final delay is
    /// in the range `[85ms, 115ms]`.
    BoundedMs {
        /// Maximum absolute jitter in milliseconds.
        max_jitter_ms: u32,
    },
}

impl JitterStrategy {
    /// Applies jitter to `base_delay_ms` using an application-provided random callback.
    ///
    /// If jitter is disabled or no callback is provided, this method returns `base_delay_ms`.
    pub fn apply(self, base_delay_ms: u32, random_fn: Option<RetryRandomFn>) -> u32 {
        let delta = match self {
            JitterStrategy::None => return base_delay_ms,
            JitterStrategy::Percentage { percent } => {
                if percent == 0 || base_delay_ms == 0 {
                    return base_delay_ms;
                }
                base_delay_ms.saturating_mul((percent.min(100)) as u32) / 100
            }
            JitterStrategy::BoundedMs { max_jitter_ms } => {
                if max_jitter_ms == 0 {
                    return base_delay_ms;
                }
                max_jitter_ms
            }
        };

        let random = match random_fn {
            Some(cb) => cb(),
            None => return base_delay_ms,
        };

        let span = delta.saturating_mul(2).saturating_add(1);
        if span == 0 {
            return base_delay_ms;
        }

        let offset = (random % span) as i64 - delta as i64;
        let jittered = base_delay_ms as i64 + offset;
        jittered.max(0) as u32
    }
}

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

    /// Returns the configured retry backoff strategy.
    pub fn retry_backoff_strategy(&self) -> BackoffStrategy {
        match self {
            ModbusConfig::Tcp(config) => config.retry_backoff_strategy,
            ModbusConfig::Serial(config) => config.retry_backoff_strategy,
        }
    }

    /// Returns the configured retry jitter strategy.
    pub fn retry_jitter_strategy(&self) -> JitterStrategy {
        match self {
            ModbusConfig::Tcp(config) => config.retry_jitter_strategy,
            ModbusConfig::Serial(config) => config.retry_jitter_strategy,
        }
    }

    /// Returns the optional application-provided random callback for jitter.
    pub fn retry_random_fn(&self) -> Option<RetryRandomFn> {
        match self {
            ModbusConfig::Tcp(config) => config.retry_random_fn,
            ModbusConfig::Serial(config) => config.retry_random_fn,
        }
    }
}

/// Parity bit configuration for serial communication.
#[derive(Debug, Clone, Copy, Default)]
pub enum Parity {
    /// No parity bit is used.
    None,
    /// Even parity: the number of 1-bits in the data plus parity bit is even.
    #[default]
    Even,
    /// Odd parity: the number of 1-bits in the data plus parity bit is odd.
    Odd,
}

/// Number of data bits per serial character.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DataBits {
    /// 5 data bits.
    Five,
    /// 6 data bits.
    Six,
    /// 7 data bits.
    Seven,
    /// 8 data bits.
    #[default]
    Eight,
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
#[derive(Debug, Clone, Copy, Default)]
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
    /// Number of data bits per character (typically `DataBits::Eight` for RTU, `DataBits::Seven` for ASCII).
    pub data_bits: DataBits,
    /// Number of stop bits (This will be recalculated before calling the transport layer).
    pub stop_bits: u8,
    /// The parity checking mode.
    pub parity: Parity,
    /// Timeout for waiting for a response in milliseconds.
    pub response_timeout_ms: u32,
    /// Number of retries for failed operations.
    pub retry_attempts: u8,
    /// Backoff strategy used when scheduling retries.
    pub retry_backoff_strategy: BackoffStrategy,
    /// Optional jitter strategy applied on top of retry backoff delay.
    pub retry_jitter_strategy: JitterStrategy,
    /// Optional application-provided random callback used by jitter.
    pub retry_random_fn: Option<RetryRandomFn>,
}

#[derive(Debug, Clone)]
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
    /// Backoff strategy used when scheduling retries.
    pub retry_backoff_strategy: BackoffStrategy,
    /// Optional jitter strategy applied on top of retry backoff delay.
    pub retry_jitter_strategy: JitterStrategy,
    /// Optional application-provided random callback used by jitter.
    pub retry_random_fn: Option<RetryRandomFn>,
}

/// The transport module defines the `Transport` trait and related types for managing Modbus TCP communication.
impl ModbusTcpConfig {
    /// Creates a new `ModbusTcpConfig` instance with the specified host and port.
    /// # Arguments
    /// * `host` - The hostname or IP address of the Modbus TCP server to connect to.
    /// * `port` - The TCP port number on which the Modbus server is listening.
    /// # Returns
    /// A new `ModbusTcpConfig` instance with the provided host and port.
    pub fn with_default_port(host: &str) -> Result<Self, MbusError> {
        let host_string: String<64> =
            String::from_str(host).map_err(|_| MbusError::BufferTooSmall)?; // Return error if host string is too long
        Self::new(&host_string, MODBUS_TCP_DEFAULT_PORT)
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
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
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
        matches!(self, TransportType::StdTcp | TransportType::CustomTcp)
    }

    /// Returns `true` if the transport type is serial (RTU or ASCII).
    pub fn is_serial_type(&self) -> bool {
        matches!(
            self,
            TransportType::StdSerial(_) | TransportType::CustomSerial(_)
        )
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
            return Err(MbusError::InvalidBroadcastAddress);
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
}

impl Default for UnitIdOrSlaveAddr {
    /// Provides a default value for initialization or error states.
    ///
    /// # ⚠️ Warning
    /// This returns `255`, which is outside the valid Modbus slave address range (1-247).
    /// It is intended to be used as a sentinel value to represent an uninitialized or
    /// invalid address state that must be handled by the application logic.
    /// This value will/should not be sent over the wire.
    fn default() -> Self {
        // 255 is in the reserved range (248-255) and serves as a safe
        // "Null" or "Error" marker in this context.
        Self(255)
    }
}

/// A trait for types that can be created from a `u8` Unit ID or Slave Address.
pub trait UidSaddrFrom {
    /// Creates an instance from a raw stored `u8` Unit ID / Slave Address.
    ///
    /// This is intended for internal reconstruction paths where the value was
    /// originally produced from a validated `UnitIdOrSlaveAddr` and later stored
    /// as a raw `u8` (for example, queue bookkeeping fields).
    ///
    /// Do not use this for external or untrusted input parsing. For that use case,
    /// use `UnitIdOrSlaveAddr::new(...)` or `TryFrom<u8>` so invalid values are
    /// surfaced as errors.
    fn from_u8(uid_saddr: u8) -> Self;
}

/// Implementation of `UidSaddrFrom` for `UnitIdOrSlaveAddr`.
impl UidSaddrFrom for UnitIdOrSlaveAddr {
    /// Creates an instance from an internal raw `u8` Unit ID / Slave Address.
    ///
    /// This helper is used in internal flows that reconstruct an address from
    /// previously validated values serialized to `u8` for storage.
    ///
    /// If an invalid raw value is encountered (which should not occur in normal
    /// operation), this returns the `Default` sentinel instead of panicking.
    /// This makes corruption visible to upper layers without crashing.
    ///
    /// For external input validation, prefer `UnitIdOrSlaveAddr::new(...)` or
    /// `TryFrom<u8>`.
    ///
    /// # Arguments
    /// * `value` - The `u8` value representing the Unit ID or Slave Address.
    ///
    /// # Returns
    /// A new `UnitIdOrSlaveAddr` instance.
    fn from_u8(value: u8) -> Self {
        UnitIdOrSlaveAddr::new(value).unwrap_or_default()
    }
}

impl From<UnitIdOrSlaveAddr> for u8 {
    /// Implementation of `From<UnitIdOrSlaveAddr>` for `u8`.
    ///
    /// This allows `UnitIdOrSlaveAddr` to be converted into a `u8` value.
    ///
    /// # Returns
    /// The raw `u8` value of the `UnitIdOrSlaveAddr`.
    ///
    fn from(val: UnitIdOrSlaveAddr) -> Self {
        val.get()
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
    type Error: Into<MbusError> + core::fmt::Debug;

    /// Compile-time capability flag for Serial-style broadcast write semantics.
    ///
    /// Set this to `true` for transport implementations that can safely apply
    /// Modbus broadcast writes (address `0`) with no response. Most transports
    /// should keep the default `false`.
    const SUPPORTS_BROADCAST_WRITES: bool = false;

    /// Compile-time transport type metadata.
    ///
    /// Every implementation must declare its transport family here.
    /// For transports whose serial mode (RTU / ASCII) is chosen at runtime,
    /// set this to a representative value (e.g. `StdSerial(SerialMode::Rtu)`)
    /// and override [`transport_type()`](Transport::transport_type) to return
    /// the actual instance mode. The compile-time value is used by the server
    /// for optimizations such as broadcast eligibility (`is_serial_type()`),
    /// while the runtime method is authoritative for framing decisions.
    const TRANSPORT_TYPE: TransportType;

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
    /// - `Ok(Vec<u8, MAX_ADU_FRAME_LEN>)`: A non-empty heapless vector containing bytes read since
    ///   the last call.
    /// - `Err(Self::Error)`: Returns `TransportError::Timeout` if no data is currently available,
    ///   or other errors if the connection is lost or hardware fails.
    ///
    /// Contract note: when no data is available in non-blocking mode, implementations must
    /// return `Err(TransportError::Timeout)` (or transport-specific equivalent) and should not
    /// return `Ok` with an empty vector.
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error>;

    /// Checks if the transport considers itself currently active and connected.
    ///
    /// Note: For connectionless or semi-connected states (like some RS-485 setups), this
    /// might continually return `true` as long as the local port is open.
    fn is_connected(&self) -> bool;
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
