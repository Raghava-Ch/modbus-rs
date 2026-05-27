//! Async raw/pre-connected upstream gateway server.
//!
//! [`AsyncRawGatewayServer`] lets you drive a gateway session from **any** source
//! that you can represent as an [`AsyncTransport`] — raw TCP sockets, UDP
//! datagrams with a custom framing adapter, shared-memory channels, Unix sockets,
//! TLS streams, or any other medium.
//!
//! Unlike [`AsyncTcpGatewayServer`], which manages its own TCP listener, this
//! server accepts a **pre-connected** transport and runs exactly one session on
//! it.  The caller is responsible for opening (and optionally re-opening) the
//! upstream connection.
//!
//! ## When to use this
//!
//! | Scenario | Recommended API |
//! |----------|-----------------|
//! | TCP upstream (standard Modbus TCP) | [`AsyncTcpGatewayServer`] |
//! | WebSocket upstream (WASM clients) | [`AsyncWsGatewayServer`] |
//! | Serial RTU/ASCII upstream | [`AsyncSerialGatewayServer`] |
//! | Any other transport (raw socket, UDP, TLS, …) | **`AsyncRawGatewayServer`** |
//!
//! [`AsyncTcpGatewayServer`]: crate::async_gateway::AsyncTcpGatewayServer
//! [`AsyncWsGatewayServer`]: crate::ws_gateway::AsyncWsGatewayServer
//! [`AsyncSerialGatewayServer`]: crate::serial_gateway::AsyncSerialGatewayServer
//!
//! ## Example — custom raw TCP framing
//!
//! ```rust,no_run
//! # #[cfg(feature = "async")]
//! # async fn example() {
//! use heapless::Vec;
//! use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
//! use mbus_core::errors::MbusError;
//! use mbus_core::transport::{AsyncTransport, ModbusConfig, TransportType};
//! use mbus_gateway::{AsyncRawGatewayServer, PassthroughRouter};
//! use mbus_network::TokioTcpTransport;
//! use std::sync::Arc;
//! use tokio::sync::Mutex;
//!
//! // Any AsyncTransport works — here we use raw TokioTcpTransport with a
//! // custom pre-negotiated connection.
//! let raw_upstream = TokioTcpTransport::connect("10.0.0.5:5020").await.unwrap();
//! let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
//! let shared_ds = Arc::new(Mutex::new(downstream));
//!
//! use mbus_gateway::NoopEventHandler;
//! use std::time::Duration;
//! let handler = Arc::new(Mutex::new(NoopEventHandler));
//! AsyncRawGatewayServer::serve(raw_upstream, PassthroughRouter, vec![shared_ds], handler, Duration::from_secs(1)).await.unwrap();
//! # }
//! ```
//!
//! ## Implementing a custom framing adapter
//!
//! Wrap any `AsyncRead + AsyncWrite` stream in your own struct and implement
//! [`AsyncTransport`]:
//!
//! ```rust,no_run
//! # #[cfg(feature = "async")]
//! # mod example {
//! use heapless::Vec;
//! use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
//! use mbus_core::errors::MbusError;
//! use mbus_core::transport::{AsyncTransport, ModbusConfig, TransportType};
//!
//! /// A UDP framing adapter that wraps raw datagrams as Modbus TCP ADUs.
//! pub struct MyUdpAdapter {
//!     // ... internal state ...
//!     connected: bool,
//! }
//!
//! impl AsyncTransport for MyUdpAdapter {
//!     const SUPPORTS_BROADCAST_WRITES: bool = true;
//!     const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;
//!
//!     fn is_connected(&self) -> bool { self.connected }
//!
//!     async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
//!         // ... write datagram ...
//!         Ok(())
//!     }
//!
//!     async fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
//!         // ... read datagram and return as Modbus-framed bytes ...
//!         Err(MbusError::Timeout)
//!     }
//! }
//! # }
//! ```

use std::future::Future;
use std::sync::Arc;

use mbus_core::transport::AsyncTransport;
use tokio::sync::Mutex;

use crate::common::log_compat::gateway_log_debug;
use crate::common::router::GatewayRoutingPolicy;
use crate::gateway_async::gateway::{AsyncGatewayError, run_async_session};

// ─────────────────────────────────────────────────────────────────────────────
// AsyncRawGatewayServer
// ─────────────────────────────────────────────────────────────────────────────

/// Async gateway server for **pre-connected or custom-framed** upstream transports.
///
/// Drives one gateway session using an already-open [`AsyncTransport`] as the
/// upstream side.  Unlike [`AsyncTcpGatewayServer`], there is no internal
/// listener — the caller supplies the open transport directly.
///
/// This makes it trivial to use any custom framing or transport layer (raw
/// sockets, UDP, TLS, Unix sockets, …) as the upstream Modbus source.
///
/// [`AsyncTcpGatewayServer`]: crate::async_gateway::AsyncTcpGatewayServer
pub struct AsyncRawGatewayServer;

impl AsyncRawGatewayServer {
    // ── serve ─────────────────────────────────────────────────────────────────

    /// Run a single gateway session on `upstream` until the session ends.
    ///
    /// The session ends when:
    /// - The upstream transport reports [`MbusError::ConnectionClosed`] or
    ///   [`MbusError::ConnectionLost`].
    /// - An unrecoverable framing error occurs.
    ///
    /// The caller decides whether to reconnect and call `serve` again.
    ///
    /// [`MbusError::ConnectionClosed`]: mbus_core::errors::MbusError::ConnectionClosed
    /// [`MbusError::ConnectionLost`]: mbus_core::errors::MbusError::ConnectionLost
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "async")]
    /// # async fn example() {
    /// use mbus_gateway::{AsyncRawGatewayServer, PassthroughRouter, NoopEventHandler};
    /// use mbus_network::TokioTcpTransport;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use tokio::sync::Mutex;
    ///
    /// let upstream = TokioTcpTransport::connect("10.0.0.5:5020").await.unwrap();
    /// let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
    ///
    /// let handler = Arc::new(Mutex::new(NoopEventHandler));
    /// AsyncRawGatewayServer::serve(
    ///     upstream, PassthroughRouter, vec![Arc::new(Mutex::new(downstream))], handler, Duration::from_secs(1),
    /// ).await.unwrap();
    /// # }
    /// ```
    pub async fn serve<US, R, DS, EVENT>(
        upstream: US,
        router: R,
        downstreams: Vec<Arc<Mutex<DS>>>,
        handler: Arc<Mutex<EVENT>>,
        response_timeout: std::time::Duration,
    ) -> Result<(), AsyncGatewayError>
    where
        US: AsyncTransport + Send + 'static,
        R: GatewayRoutingPolicy + Send + Sync + 'static,
        DS: AsyncTransport + Send + 'static,
        EVENT: crate::common::event::GatewayEventHandler + Send + 'static,
    {
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);
        gateway_log_debug!("raw upstream gateway session started");
        run_async_session(upstream, router, downstreams, handler, response_timeout)
            .await
            .map_err(AsyncGatewayError::Modbus)
    }

    // ── serve_with_shutdown ───────────────────────────────────────────────────

    /// Run a single gateway session until it ends **or** `shutdown` resolves.
    ///
    /// Use [`GatewayShutdown`] for a cancellation-token-like interface:
    ///
    /// ```rust,no_run
    /// # #[cfg(feature = "async")]
    /// # async fn example() {
    /// use mbus_gateway::{AsyncRawGatewayServer, GatewayShutdown, PassthroughRouter, NoopEventHandler};
    /// use mbus_network::TokioTcpTransport;
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use tokio::sync::Mutex;
    ///
    /// let (token, shutdown) = GatewayShutdown::new();
    /// tokio::spawn(async move {
    ///     // Cancel after 60 s or from a UI action.
    ///     tokio::time::sleep(std::time::Duration::from_secs(60)).await;
    ///     token.cancel();
    /// });
    ///
    /// let upstream = TokioTcpTransport::connect("10.0.0.5:5020").await.unwrap();
    /// let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
    ///
    /// let handler = Arc::new(Mutex::new(NoopEventHandler));
    /// AsyncRawGatewayServer::serve_with_shutdown(
    ///     upstream, PassthroughRouter, vec![Arc::new(Mutex::new(downstream))], handler, Duration::from_secs(1), shutdown,
    /// ).await.unwrap();
    /// # }
    /// ```
    ///
    /// [`GatewayShutdown`]: crate::shutdown::GatewayShutdown
    pub async fn serve_with_shutdown<US, R, DS, EVENT, F>(
        upstream: US,
        router: R,
        downstreams: Vec<Arc<Mutex<DS>>>,
        handler: Arc<Mutex<EVENT>>,
        response_timeout: std::time::Duration,
        shutdown: F,
    ) -> Result<(), AsyncGatewayError>
    where
        US: AsyncTransport + Send + 'static,
        R: GatewayRoutingPolicy + Send + Sync + 'static,
        DS: AsyncTransport + Send + 'static,
        EVENT: crate::common::event::GatewayEventHandler + Send + 'static,
        F: Future<Output = ()>,
    {
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);

        gateway_log_debug!("raw upstream gateway session started (with shutdown)");

        tokio::pin!(shutdown);
        tokio::select! {
            result = run_async_session(upstream, router, downstreams, handler, response_timeout) => {
                result.map_err(AsyncGatewayError::Modbus)
            }
            _ = &mut shutdown => {
                gateway_log_debug!("raw upstream gateway received shutdown signal");
                Ok(())
            }
        }
    }
}
