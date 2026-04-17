//! The core `Transport` and `TimeKeeper` traits.

use crate::{data_unit::common::MAX_ADU_FRAME_LEN, errors::MbusError};
use heapless::Vec;

use super::{config::ModbusConfig, error::TransportType};

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
    /// Returns the current time in milliseconds.
    ///
    /// In a real `no_std` environment, this would come from a hardware timer.
    fn current_millis(&self) -> u64;
}
