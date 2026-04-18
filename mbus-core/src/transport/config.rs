//! Transport configuration types: TCP and Serial.

use core::str::FromStr;

use crate::errors::MbusError;
use heapless::String;

use super::retry::{BackoffStrategy, JitterStrategy, RetryRandomFn};

/// The default TCP port for Modbus communication.
const MODBUS_TCP_DEFAULT_PORT: u16 = 502;

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

/// Serial framing mode.
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
    Custom(u32),
}

/// Configuration parameters for establishing a Modbus Serial connection.
#[derive(Debug)]
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

/// Configuration parameters for establishing a Modbus TCP connection.
#[derive(Debug, Clone)]
pub struct ModbusTcpConfig {
    /// The hostname or IP address of the Modbus TCP server to connect to.
    pub host: heapless::String<64>,
    /// The TCP port number on which the Modbus server is listening (default is typically 502).
    pub port: u16,
    /// Timeout for establishing a connection in milliseconds.
    pub connection_timeout_ms: u32,
    /// Timeout for waiting for a response in milliseconds.
    pub response_timeout_ms: u32,
    /// Number of retry attempts for failed operations.
    pub retry_attempts: u8,
    /// Backoff strategy used when scheduling retries.
    pub retry_backoff_strategy: BackoffStrategy,
    /// Optional jitter strategy applied on top of retry backoff delay.
    pub retry_jitter_strategy: JitterStrategy,
    /// Optional application-provided random callback used by jitter.
    pub retry_random_fn: Option<RetryRandomFn>,
}

impl ModbusTcpConfig {
    /// Creates a new `ModbusTcpConfig` using the default Modbus TCP port (502).
    pub fn with_default_port(host: &str) -> Result<Self, MbusError> {
        let host_string: String<64> =
            String::from_str(host).map_err(|_| MbusError::BufferTooSmall)?;
        Self::new(&host_string, MODBUS_TCP_DEFAULT_PORT)
    }

    /// Creates a new `ModbusTcpConfig` with the specified host and port.
    pub fn new(host: &str, port: u16) -> Result<Self, MbusError> {
        let host_string = String::from_str(host).map_err(|_| MbusError::BufferTooSmall)?;
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

/// Top-level configuration for Modbus communication, supporting different transport layers.
#[derive(Debug)]
pub enum ModbusConfig {
    /// Configuration for Modbus TCP/IP.
    Tcp(ModbusTcpConfig),
    /// Configuration for Modbus Serial (RTU or ASCII).
    Serial(ModbusSerialConfig),
}

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
