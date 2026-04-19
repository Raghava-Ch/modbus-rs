//! Async transport abstraction — parallel to the sync [`Transport`](super::Transport) trait.
//!
//! Enabled by the `async` feature flag. No new dependencies are required; `async fn`
//! desugars to `core::future::Future` which is always available in `no_std` environments.

use crate::data_unit::common::MAX_ADU_FRAME_LEN;
use crate::errors::MbusError;
use crate::transport::TransportType;
use core::future::Future;
use heapless::Vec;

/// Async transport abstraction for Modbus communication.
///
/// This trait is the async parallel of the sync [`Transport`](super::Transport) trait.
/// Implementations live in `mbus-network` (`TokioTcpTransport`) and `mbus-serial`
/// (`TokioRtuTransport`, `TokioAsciiTransport`) behind their respective `async` feature flags.
///
/// # Framing contract
///
/// Unlike the sync `Transport::recv()` which returns whatever bytes are available,
/// `AsyncTransport::recv()` **must not return until exactly one complete ADU is ready**:
///
/// - **TCP**: reads the 6-byte MBAP prefix, parses the length field, then reads exactly that
///   many remaining bytes. Always returns a complete, valid-length frame.
/// - **Serial RTU**: accumulates bytes and returns when the inter-frame silence timer fires
///   (3.5 character times). The timer resets on every received byte.
///   The timer is only started after the first byte arrives — silence before any data
///   is not treated as a frame boundary.
/// - **Serial ASCII**: accumulates bytes until the `\r\n` terminator is found.
///
/// # Send bounds
///
/// Both `send` and `recv` return futures that are `Send`, enabling their use with
/// `tokio::spawn` without boxing. Implementations using `async fn` syntax are accepted
/// by the compiler as long as all captured state is `Send`.
pub trait AsyncTransport: Send {
    /// Send a complete Modbus ADU frame over the transport.
    ///
    /// Implementations must ensure all bytes are written before returning.
    fn send<'a>(
        &'a mut self,
        adu: &'a [u8],
    ) -> impl Future<Output = Result<(), MbusError>> + Send + 'a;

    /// Receive exactly one complete Modbus ADU frame.
    ///
    /// Suspends the caller until a full frame is available. See the
    /// [framing contract](AsyncTransport#framing-contract) for details per transport type.
    fn recv(
        &mut self,
    ) -> impl Future<Output = Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError>> + Send + '_;

    /// Runtime transport type — used by the server session for framing decisions
    /// (e.g. TCP vs RTU vs ASCII ADU layout, broadcast eligibility).
    fn transport_type(&self) -> TransportType;

    /// Whether the transport currently has an active connection.
    fn is_connected(&self) -> bool;
}
