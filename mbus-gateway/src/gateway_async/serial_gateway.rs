//! Async serial upstream gateway server.
//!
//! [`AsyncSerialGatewayServer`] accepts a single async serial connection from an
//! upstream RTU or ASCII Modbus master (e.g. a PLC or SCADA system connected via
//! RS-485) and forwards its requests to one or more downstream [`AsyncTransport`]
//! channels.
//!
//! This is the async counterpart to the sync serial-upstream path in
//! [`GatewayServices`](crate::GatewayServices).  Use it when:
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
use std::sync::{Arc, RwLock};
use std::time::Duration;
use std::str::FromStr;

use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::transport::{
    AsyncTransport, BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig,
    ModbusSerialConfig, Parity, SerialMode, TransportType,
};
use mbus_core::data_unit::common::{decompile_adu_frame, compile_adu_frame};
use tokio::sync::Mutex;

use crate::common::event::GatewayEventHandler;
use crate::common::log_compat::{gateway_log_debug, gateway_log_trace, gateway_log_warn};
use crate::common::router::{GatewayRoutingPolicy, UnitRouteTable};
use crate::gateway_async::downstream::GatewayTransport;
use crate::gateway_async::gateway::{AsyncGatewayError, send_async_exception};
use mbus_serial::{TokioAsciiTransport, TokioRtuTransport};

/// Configuration for the upstream serial gateway server.
#[derive(Debug, Clone)]
pub struct SerialGatewayConfig {
    pub port: String,
    pub mode: SerialMode,
    pub baud_rate: BaudRate,
    pub data_bits: DataBits,
    pub stop_bits: u8,
    pub parity: Parity,
    pub response_timeout: Duration,
}

/// Gateway error type alias.
pub type GatewayError = AsyncGatewayError;

// ─────────────────────────────────────────────────────────────────────────────
// AsyncSerialGatewayServer
// ─────────────────────────────────────────────────────────────────────────────

/// Async Modbus serial upstream gateway.
///
/// Runs a single gateway session where the **upstream** is a serial Modbus
/// master (RTU or ASCII) configured with [`SerialGatewayConfig`], and the
/// **downstreams** are [`GatewayTransport`] channels (TCP, RTU, or ASCII).
pub struct AsyncSerialGatewayServer;

impl AsyncSerialGatewayServer {
    /// Run the gateway session until it ends naturally **or** `shutdown_future` resolves.
    ///
    /// Under the hood, this method:
    /// 1. Opens the specified serial port asynchronously via `tokio-serial`.
    /// 2. Delineates packets:
    ///    - **RTU mode**: Using a 3.5 character timeout, verifying CRC-16.
    ///    - **ASCII mode**: Reading until `\r\n` (CRLF) delimiters, verifying LRC check.
    /// 3. Parses the Modbus Slave Address (Unit ID) and request PDU.
    /// 4. Looks up `router.read()` to match the target downstream channel.
    /// 5. Locks the matching downstream channel's transport mutex, translates the PDU,
    ///    waits for the response, and writes the formatted response back to the serial line.
    /// 6. Triggers observer callbacks on `handler` to feed metrics and traffic logs.
    pub async fn serve_with_shutdown<F, const MAX_ROUTES: usize>(
        cfg: SerialGatewayConfig,
        router: Arc<RwLock<UnitRouteTable<MAX_ROUTES>>>,
        downstreams: Vec<Arc<Mutex<GatewayTransport>>>,
        handler: Arc<Mutex<dyn GatewayEventHandler + Send>>,
        shutdown_future: F,
    ) -> Result<(), GatewayError>
    where
        F: Future<Output = ()> + Send + 'static,
    {
        gateway_log_debug!("serial upstream gateway session started with config: {:?}", cfg);

        // 1. Open the specified serial port asynchronously via `tokio-serial` / Tokio transports.
        let port_path = heapless::String::<64>::from_str(&cfg.port)
            .map_err(|_| AsyncGatewayError::Modbus(MbusError::InvalidConfiguration))?;

        let modbus_cfg = ModbusConfig::Serial(ModbusSerialConfig {
            port_path,
            mode: cfg.mode,
            baud_rate: cfg.baud_rate,
            data_bits: cfg.data_bits,
            stop_bits: cfg.stop_bits,
            parity: cfg.parity,
            response_timeout_ms: cfg.response_timeout.as_millis() as u32,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        tokio::pin!(shutdown_future);

        match cfg.mode {
            SerialMode::Rtu => {
                let mut upstream = TokioRtuTransport::new(&modbus_cfg)
                    .map_err(AsyncGatewayError::Modbus)?;
                Self::run_loop(
                    &mut upstream,
                    true,
                    router,
                    downstreams,
                    handler,
                    cfg.response_timeout,
                    shutdown_future,
                )
                .await
            }
            SerialMode::Ascii => {
                let mut upstream = TokioAsciiTransport::new(&modbus_cfg)
                    .map_err(AsyncGatewayError::Modbus)?;
                Self::run_loop(
                    &mut upstream,
                    false,
                    router,
                    downstreams,
                    handler,
                    cfg.response_timeout,
                    shutdown_future,
                )
                .await
            }
        }
    }

    async fn run_loop<T, F, const MAX_ROUTES: usize>(
        upstream: &mut T,
        is_rtu: bool,
        router: Arc<RwLock<UnitRouteTable<MAX_ROUTES>>>,
        downstreams: Vec<Arc<Mutex<GatewayTransport>>>,
        handler: Arc<Mutex<dyn GatewayEventHandler + Send>>,
        response_timeout: Duration,
        mut shutdown_future: std::pin::Pin<&mut F>,
    ) -> Result<(), GatewayError>
    where
        T: AsyncTransport + Send,
        F: Future<Output = ()> + Send + 'static,
    {
        let transport_type = if is_rtu {
            TransportType::StdSerial(SerialMode::Rtu)
        } else {
            TransportType::StdSerial(SerialMode::Ascii)
        };

        loop {
            // 2. Delineate packets using a 3.5 character timeout or CRLF delimiter
            let frame = tokio::select! {
                _ = &mut shutdown_future => {
                    gateway_log_debug!("serial upstream gateway received shutdown signal");
                    return Ok(());
                }
                recv_res = upstream.recv() => {
                    match recv_res {
                        Ok(f) => f,
                        Err(MbusError::ConnectionClosed) | Err(MbusError::ConnectionLost) => {
                            gateway_log_debug!("upstream disconnected");
                            break;
                        }
                        Err(e) => {
                            gateway_log_debug!("upstream recv error: {:?}", e);
                            return Err(AsyncGatewayError::Modbus(e));
                        }
                    }
                }
            };

            #[cfg(feature = "traffic")]
            {
                let mut h = handler.lock().await;
                h.on_upstream_rx(0, &frame);
            }

            gateway_log_trace!(
                "upstream rx: {} bytes (type={:?})",
                frame.len(),
                transport_type
            );

            // Delineating RTU/ASCII and verifying CRC-16 / LRC checksums
            let msg = match decompile_adu_frame(&frame, transport_type) {
                Ok(m) => m,
                Err(e) => {
                    gateway_log_debug!("upstream frame checksum or parsing failure: {:?}", e);
                    continue;
                }
            };

            // 3. Parse the Modbus Slave Address (Unit ID) and the request PDU.
            let unit = msg.unit_id_or_slave_addr();
            let upstream_txn = msg.transaction_id();
            let fc = msg.pdu.function_code();

            // 4. Lookup `router.read()` to match the target downstream channel.
            let route_result = {
                let r = router.read().unwrap();
                r.route(unit)
            };

            let channel_idx = match route_result {
                Some(idx) => {
                    let mut h = handler.lock().await;
                    h.on_forward(0, unit, idx);
                    idx
                }
                None => {
                    gateway_log_debug!("routing miss for unit={}", unit.get());
                    let _ = send_async_exception(
                        upstream,
                        upstream_txn,
                        unit,
                        fc,
                        ExceptionCode::ServerDeviceFailure,
                        transport_type,
                    )
                    .await;
                    continue;
                }
            };

            if channel_idx >= downstreams.len() {
                gateway_log_warn!(
                    "routing index out of bounds: {} (available downstreams: {})",
                    channel_idx,
                    downstreams.len()
                );
                let _ = send_async_exception(
                    upstream,
                    upstream_txn,
                    unit,
                    fc,
                    ExceptionCode::ServerDeviceFailure,
                    transport_type,
                )
                .await;
                continue;
            }

            let downstream_unit = {
                let r = router.read().unwrap();
                r.rewrite(unit)
            };

            // 5. Lock the `downstreams[channel_idx]` transport mutex, execute PDU translation, wait for response, and write the formatted Modbus response frame back to the serial line.
            let ds_adu = match compile_adu_frame(
                0,
                downstream_unit.get(),
                msg.pdu.clone(),
                TransportType::StdTcp,
            ) {
                Ok(adu) => adu,
                Err(e) => {
                    gateway_log_debug!("failed to encode downstream ADU: {:?}", e);
                    continue;
                }
            };

            #[cfg(feature = "traffic")]
            {
                let mut h = handler.lock().await;
                h.on_downstream_tx(channel_idx, &ds_adu);
            }

            let response_bytes = {
                let mut ds = downstreams[channel_idx].lock().await;

                if let Err(e) = ds.send(&ds_adu).await {
                    gateway_log_debug!("downstream send error: {:?}", e);
                    continue;
                }

                match tokio::time::timeout(response_timeout, ds.recv()).await {
                    Ok(Ok(b)) => b,
                    Ok(Err(e)) => {
                        gateway_log_debug!("downstream recv error: {:?}", e);
                        continue;
                    }
                    Err(_) => {
                        gateway_log_debug!("downstream recv timeout");
                        let mut h = handler.lock().await;
                        h.on_downstream_timeout(0, 0);
                        let _ = send_async_exception(
                            upstream,
                            upstream_txn,
                            unit,
                            fc,
                            ExceptionCode::GatewayTargetDeviceFailedToRespond,
                            transport_type,
                        )
                        .await;
                        continue;
                    }
                }
            };

            #[cfg(feature = "traffic")]
            {
                let mut h = handler.lock().await;
                h.on_downstream_rx(0, channel_idx, &response_bytes);
            }

            let response_msg = match decompile_adu_frame(&response_bytes, TransportType::StdTcp) {
                Ok(m) => m,
                Err(e) => {
                    gateway_log_debug!("downstream response parse error: {:?}", e);
                    continue;
                }
            };

            let us_adu = match compile_adu_frame(
                upstream_txn,
                unit.get(),
                response_msg.pdu.clone(),
                transport_type,
            ) {
                Ok(adu) => adu,
                Err(e) => {
                    gateway_log_debug!("failed to encode upstream response: {:?}", e);
                    continue;
                }
            };

            if let Err(e) = upstream.send(&us_adu).await {
                gateway_log_debug!("upstream send error: {:?}", e);
                break;
            }

            // 6. Trigger callback methods on `handler`
            #[cfg(feature = "traffic")]
            {
                let mut h = handler.lock().await;
                h.on_upstream_tx(0, &us_adu);
            }

            {
                let mut h = handler.lock().await;
                h.on_response_returned(0, upstream_txn);
            }

            tokio::time::sleep(Duration::from_micros(10)).await;
        }

        {
            let mut h = handler.lock().await;
            h.on_upstream_disconnect(0);
        }

        Ok(())
    }
}
