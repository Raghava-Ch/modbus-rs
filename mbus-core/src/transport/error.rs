//! Transport-layer error and type-classification types.

#[cfg(all(feature = "defmt-format", target_os = "none"))]
use defmt;

use crate::errors::MbusError;

use super::config::SerialMode;

/// Represents errors that can occur at the Modbus transport layer.
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
    /// Invalid configuration.
    InvalidConfiguration,
    /// The transport detected a protocol-level framing or timing violation
    /// (e.g., inter-character gap exceeding t1.5 on serial RTU).
    FramingError,
}

#[cfg(all(feature = "defmt-format", target_os = "none"))]
impl defmt::Format for TransportError {
    fn format(&self, f: defmt::Formatter) {
        match self {
            TransportError::ConnectionFailed => defmt::write!(f, "Connection failed"),
            TransportError::ConnectionClosed => defmt::write!(f, "Connection closed"),
            TransportError::IoError => defmt::write!(f, "I/O error"),
            TransportError::Timeout => defmt::write!(f, "Timeout"),
            TransportError::BufferTooSmall => defmt::write!(f, "Buffer too small"),
            TransportError::Unexpected => defmt::write!(f, "An unexpected error occurred"),
            TransportError::InvalidConfiguration => defmt::write!(f, "Invalid configuration"),
            TransportError::FramingError => {
                defmt::write!(f, "Framing error: protocol timing violation")
            }
        }
    }
}

#[cfg(feature = "error-trait")]
impl core::fmt::Display for TransportError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TransportError::ConnectionFailed => write!(f, "Connection failed"),
            TransportError::ConnectionClosed => write!(f, "Connection closed"),
            TransportError::IoError => write!(f, "I/O error"),
            TransportError::Timeout => write!(f, "Timeout"),
            TransportError::BufferTooSmall => write!(f, "Buffer too small"),
            TransportError::Unexpected => write!(f, "Unexpected error"),
            TransportError::InvalidConfiguration => write!(f, "Invalid configuration"),
            TransportError::FramingError => write!(f, "Framing error"),
        }
    }
}

#[cfg(feature = "error-trait")]
impl core::error::Error for TransportError {}

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
            TransportError::FramingError => MbusError::FramingError,
        }
    }
}

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
