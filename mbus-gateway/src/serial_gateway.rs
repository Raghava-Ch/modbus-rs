//! Async serial upstream gateway server.
//!
//! [`AsyncSerialGatewayServer`] accepts a single async serial connection from an
//! upstream RTU or ASCII Modbus master (e.g. a PLC or SCADA system connected via
//! RS-485) and forwards its requests to one or more downstream [`AsyncTransport`]
//! channels.
//!
//! This is the async counterpart to the sync serial-upstream path in
//! [`GatewayServices`](crate::services::GatewayServices).  Use it when:
//!
//! - Your **upstream** is a serial Modbus master (physical RS-485/RS-232 line),
//!   **and** your **downstreams** are async (e.g. TCP/IP Modbus slaves over
//!   `TokioTcpTransport`).
//!
//! ## Feature flags
//!
//! | Feature              | What it enables                                |
//! |----------------------|------------------------------------------------|
//! | `serial-rtu-async`   | [`AsyncSerialGatewayServer`] for RTU masters   |
//! | `serial-ascii-async` | [`AsyncSerialGatewayServer`] for ASCII masters |
//!
//! ## Example (RTU master → TCP downstream)
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "serial-rtu-async", feature = "async"))]
//! # async fn example() {
//! use mbus_core::transport::{
//!     BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
//!     BackoffStrategy, Parity, SerialMode, UnitIdOrSlaveAddr,
//! };
//! use mbus_gateway::{AsyncSerialGatewayServer, UnitRouteTable, NoopEventHandler};
//! use mbus_network::TokioTcpTransport;
//! use mbus_serial::TokioRtuTransport;
//! use std::sync::Arc;
//! use std::time::Duration;
//! use tokio::sync::Mutex;
//!
//! let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig {
//!     port_path: "/dev/ttyUSB0".try_into().unwrap(),
//!     mode: SerialMode::Rtu,
//!     baud_rate: BaudRate::Baud19200,
//!     data_bits: DataBits::Eight,
//!     stop_bits: 1,
//!     parity: Parity::None,
//!     response_timeout_ms: 1000,
//!     retry_attempts: 0,
//!     retry_backoff_strategy: BackoffStrategy::Immediate,
//!     retry_jitter_strategy: JitterStrategy::None,
//!     retry_random_fn: None,
//! });
//! let rtu_upstream = TokioRtuTransport::new(&serial_cfg).unwrap();
//!
//! let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
//! let shared = Arc::new(Mutex::new(downstream));
//!
//! let mut router: UnitRouteTable<8> = UnitRouteTable::new();
//! router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
//!
//! // Serve the serial upstream master forever.
//! let handler = Arc::new(Mutex::new(NoopEventHandler));
//! AsyncSerialGatewayServer::serve(rtu_upstream, router, vec![shared], handler, Duration::from_secs(1)).await.unwrap();
//! # }
//! ```
//!
//! ## Graceful shutdown
//!
//! Use [`AsyncSerialGatewayServer::serve_with_shutdown`] and pass any `Future<Output = ()>`
//! as the shutdown signal:
//!
//! ```rust,no_run
//! # #[cfg(all(feature = "serial-rtu-async", feature = "async"))]
//! # async fn example() {
//! # use mbus_gateway::{AsyncSerialGatewayServer, UnitRouteTable, GatewayShutdown, NoopEventHandler};
//! # use mbus_core::transport::{BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, BackoffStrategy, Parity, SerialMode, UnitIdOrSlaveAddr};
//! # use mbus_serial::TokioRtuTransport;
//! # use mbus_network::TokioTcpTransport;
//! # use std::sync::Arc;
//! # use std::time::Duration;
//! # use tokio::sync::Mutex;
//! # let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig { port_path: "/dev/ttyUSB0".try_into().unwrap(), mode: SerialMode::Rtu, baud_rate: BaudRate::Baud19200, data_bits: DataBits::Eight, stop_bits: 1, parity: Parity::None, response_timeout_ms: 1000, retry_attempts: 0, retry_backoff_strategy: BackoffStrategy::Immediate, retry_jitter_strategy: JitterStrategy::None, retry_random_fn: None });
//! # let rtu_upstream = TokioRtuTransport::new(&serial_cfg).unwrap();
//! # let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
//! # let shared = Arc::new(Mutex::new(downstream));
//! # let mut router: UnitRouteTable<8> = UnitRouteTable::new();
//! let (token, shutdown) = GatewayShutdown::new();
//!
//! // In another task: signal shutdown.
//! tokio::spawn(async move {
//!     tokio::time::sleep(std::time::Duration::from_secs(30)).await;
//!     token.cancel();
//! });
//!
//! let handler = Arc::new(Mutex::new(NoopEventHandler));
//! AsyncSerialGatewayServer::serve_with_shutdown(
//!     rtu_upstream, router, vec![shared], handler, Duration::from_secs(1), shutdown,
//! ).await.unwrap();
//! # }
//! ```

use std::future::Future;
use std::sync::Arc;

use mbus_core::transport::AsyncTransport;
use tokio::sync::Mutex;

use crate::async_gateway::{AsyncGatewayError, run_async_session};
use crate::log_compat::gateway_log_debug;
use crate::router::GatewayRoutingPolicy;

// ─────────────────────────────────────────────────────────────────────────────
// AsyncSerialGatewayServer
// ─────────────────────────────────────────────────────────────────────────────

/// Async Modbus serial upstream gateway.
///
/// Runs a single gateway session where the **upstream** is a serial Modbus
/// master (RTU or ASCII) connected via `TokioRtuTransport` /
/// `TokioAsciiTransport`, and the **downstreams** are any [`AsyncTransport`]
/// channels (TCP, more serial ports, custom, …).
///
/// Unlike [`AsyncTcpGatewayServer`] or [`AsyncWsGatewayServer`], which run a
/// listener loop accepting multiple concurrent upstream clients, this server runs
/// a **single session** — serial links are point-to-point by nature.
///
/// The session automatically restarts when the serial port reports a connection
/// error (e.g., device unplugged) if `serve` is called in a loop by the caller.
///
/// [`AsyncTcpGatewayServer`]: crate::async_gateway::AsyncTcpGatewayServer
/// [`AsyncWsGatewayServer`]: crate::ws_gateway::AsyncWsGatewayServer
pub struct AsyncSerialGatewayServer;

impl AsyncSerialGatewayServer {
    // ── serve ─────────────────────────────────────────────────────────────────

    /// Run the gateway session on the given serial upstream transport until an
    /// unrecoverable error occurs.
    ///
    /// `upstream` is the open, ready-to-use serial transport (e.g.
    /// `TokioRtuTransport::new(&config)?`).  The gateway forwards every
    /// upstream request to the appropriate downstream channel according to
    /// `router` and returns the response.
    ///
    /// # When to restart
    ///
    /// If the serial master resets or the cable is unplugged, `serve` returns
    /// `Err(AsyncGatewayError::Modbus(_))`.  The caller can reconstruct the
    /// transport and call `serve` again:
    ///
    /// ```rust,no_run
    /// # async fn example() {}
    /// // loop { match AsyncSerialGatewayServer::serve(...).await { ... } }
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
        EVENT: crate::event::GatewayEventHandler + Send + 'static,
    {
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);

        gateway_log_debug!("serial upstream gateway session started");
        if let Err(e) =
            run_async_session(upstream, router, downstreams, handler, response_timeout).await
        {
            gateway_log_debug!("serial upstream session ended: {:?}", e);
            return Err(AsyncGatewayError::Modbus(e));
        }
        Ok(())
    }

    // ── serve_with_shutdown ───────────────────────────────────────────────────

    /// Run the gateway session until it ends naturally **or** `shutdown` resolves.
    ///
    /// Pass any `Future<Output = ()>` as `shutdown`.  The easiest way is to use
    /// [`GatewayShutdown`]:
    ///
    /// ```rust,no_run
    /// # async fn example() {}
    /// // let (token, shutdown) = GatewayShutdown::new();
    /// // token.cancel(); // from another task
    /// // AsyncSerialGatewayServer::serve_with_shutdown(upstream, router, ds, shutdown).await?;
    /// ```
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
        EVENT: crate::event::GatewayEventHandler + Send + 'static,
        F: Future<Output = ()>,
    {
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);

        gateway_log_debug!("serial upstream gateway session started (with shutdown)");

        tokio::pin!(shutdown);
        tokio::select! {
            result = run_async_session(upstream, router, downstreams, handler, response_timeout) => {
                match result {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        gateway_log_debug!("serial upstream session ended: {:?}", e);
                        Err(AsyncGatewayError::Modbus(e))
                    }
                }
            }
            _ = &mut shutdown => {
                gateway_log_debug!("serial upstream gateway received shutdown signal");
                Ok(())
            }
        }
    }
}
