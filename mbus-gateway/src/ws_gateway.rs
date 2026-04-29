//! Async WebSocket gateway server.
//!
//! [`AsyncWsGatewayServer`] binds a TCP port, performs the WebSocket handshake
//! for each incoming connection, and then hands the session off to the same
//! generic [`run_async_session`] loop used by [`AsyncTcpGatewayServer`].
//! The downstream side is unchanged — any [`AsyncTransport`] (raw TCP, RTU,
//! ASCII) can be used.
//!
//! ## Why WebSocket?
//!
//! Browsers can only open raw sockets via the WebSocket API.  A WASM
//! [`WasmModbusClient`] therefore communicates with the gateway over WebSocket,
//! while the gateway uses a plain TCP (or serial) connection to reach the
//! downstream Modbus devices.
//!
//! The browser-side client already constructs complete Modbus TCP ADUs (MBAP +
//! PDU) and wraps each one in a binary WebSocket message.  The gateway unwraps
//! each message and forwards the ADU as-is — no re-framing is required.
//!
//! ## Extended features
//!
//! [`WsGatewayConfig`] lets you tune the following:
//!
//! | Feature | Field | Default |
//! |---------|-------|---------|
//! | Idle-session timeout | [`idle_timeout`] | `None` (no timeout) |
//! | Session concurrency cap | [`max_sessions`] | `0` = unlimited |
//! | Require `"modbus"` WS subprotocol | [`require_modbus_subprotocol`] | `false` |
//! | Origin allowlist | [`allowed_origins`] | `[]` = allow all |
//!
//! [`idle_timeout`]: WsGatewayConfig::idle_timeout
//! [`max_sessions`]: WsGatewayConfig::max_sessions
//! [`require_modbus_subprotocol`]: WsGatewayConfig::require_modbus_subprotocol
//! [`allowed_origins`]: WsGatewayConfig::allowed_origins
//! [`AsyncTcpGatewayServer`]: crate::async_gateway::AsyncTcpGatewayServer
//! [`WasmModbusClient`]: https://docs.rs/mbus-ffi
//! [`run_async_session`]: crate::async_gateway::run_async_session

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use mbus_core::errors::MbusError;
use mbus_core::transport::{AsyncTransport, TransportType};
use mbus_network::WsUpstreamTransport;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::{Mutex, Semaphore};

use crate::async_gateway::{run_async_session, AsyncGatewayError};
use crate::log_compat::{gateway_log_debug, gateway_log_warn};
use crate::router::GatewayRoutingPolicy;

// ─────────────────────────────────────────────────────────────────────────────
// WsGatewayConfig
// ─────────────────────────────────────────────────────────────────────────────

/// Configuration for [`AsyncWsGatewayServer`].
///
/// All fields have sensible defaults via [`WsGatewayConfig::default()`].  Build
/// the config with struct literal syntax and fill in only what you need:
///
/// ```rust,no_run
/// use mbus_gateway::WsGatewayConfig;
/// use std::time::Duration;
///
/// let cfg = WsGatewayConfig {
///     idle_timeout: Some(Duration::from_secs(30)),
///     max_sessions: 64,
///     require_modbus_subprotocol: true,
///     allowed_origins: vec!["https://example.com".to_string()],
/// };
/// ```
#[derive(Debug, Clone)]
pub struct WsGatewayConfig {
    /// Maximum time to wait between frames before closing an idle session.
    ///
    /// If a connected browser tab sends no Modbus requests for longer than
    /// this duration the gateway drops the WebSocket connection cleanly.
    /// `None` (the default) disables the timeout entirely.
    pub idle_timeout: Option<Duration>,

    /// Maximum number of concurrent WebSocket sessions.
    ///
    /// When the limit is reached, new TCP connections are accepted at the OS
    /// level (so the kernel does not queue them at the SYN stage) but the
    /// WebSocket handshake is immediately rejected.
    ///
    /// `0` means unlimited (default).
    pub max_sessions: usize,

    /// Require the browser to declare the `"modbus"` WebSocket subprotocol.
    ///
    /// When `true` the gateway inspects the `Sec-WebSocket-Protocol` header
    /// during the HTTP upgrade handshake.  Connections that do not include
    /// `"modbus"` in the protocol list receive an HTTP `400 Bad Request`
    /// response.  This prevents accidental connections from ordinary browsers
    /// navigating to the gateway port.
    ///
    /// Default: `false`.
    pub require_modbus_subprotocol: bool,

    /// List of allowed `Origin` header values.
    ///
    /// When non-empty, connections whose `Origin` header is missing or does not
    /// appear in this list receive an HTTP `403 Forbidden` response.
    ///
    /// Default: empty = allow all origins.
    pub allowed_origins: Vec<String>,
}

impl Default for WsGatewayConfig {
    fn default() -> Self {
        Self {
            idle_timeout: None,
            max_sessions: 0,
            require_modbus_subprotocol: false,
            allowed_origins: Vec::new(),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// IdleTimeoutTransport
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps any [`AsyncTransport`] and enforces an idle timeout on `recv()`.
///
/// When the timeout fires, `recv()` returns [`MbusError::ConnectionClosed`] so
/// that the owning session loop exits cleanly (same path as a normal client
/// disconnect).
struct IdleTimeoutTransport<T: AsyncTransport> {
    inner: T,
    timeout: Duration,
}

impl<T: AsyncTransport + Send> AsyncTransport for IdleTimeoutTransport<T> {
    const SUPPORTS_BROADCAST_WRITES: bool = T::SUPPORTS_BROADCAST_WRITES;
    const TRANSPORT_TYPE: TransportType = T::TRANSPORT_TYPE;

    fn is_connected(&self) -> bool {
        self.inner.is_connected()
    }

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.inner.send(adu).await
    }

    async fn recv(
        &mut self,
    ) -> Result<heapless::Vec<u8, { mbus_core::data_unit::common::MAX_ADU_FRAME_LEN }>, MbusError>
    {
        match tokio::time::timeout(self.timeout, self.inner.recv()).await {
            Ok(result) => result,
            Err(_elapsed) => {
                gateway_log_debug!("idle timeout — closing session");
                Err(MbusError::ConnectionClosed)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AsyncWsGatewayServer
// ─────────────────────────────────────────────────────────────────────────────

/// Async Modbus WebSocket gateway.
///
/// Accepts WebSocket connections from browser-side [`WasmModbusClient`]
/// instances and forwards each Modbus request to a downstream [`AsyncTransport`]
/// (raw TCP, RTU, ASCII, …) using the same session logic as
/// [`AsyncTcpGatewayServer`].
///
/// Each upstream session runs in a dedicated `tokio::spawn`-ed task.
/// Downstream channels are shared between sessions via
/// `Arc<tokio::sync::Mutex<DS>>`.
///
/// # Example
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use mbus_gateway::{AsyncWsGatewayServer, WsGatewayConfig, UnitRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
/// use mbus_network::TokioTcpTransport;
/// use std::sync::Arc;
/// use std::time::Duration;
/// use tokio::sync::Mutex;
///
/// let mut router: UnitRouteTable<4> = UnitRouteTable::new();
/// router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
///
/// let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
/// let shared = Arc::new(Mutex::new(downstream));
///
/// let config = WsGatewayConfig {
///     idle_timeout: Some(Duration::from_secs(30)),
///     max_sessions: 64,
///     require_modbus_subprotocol: true,
///     allowed_origins: vec!["https://example.com".to_string()],
/// };
///
/// AsyncWsGatewayServer::serve("0.0.0.0:8502", config, router, vec![shared]).await?;
/// # Ok(())
/// # }
/// ```
///
/// [`AsyncTcpGatewayServer`]: crate::async_gateway::AsyncTcpGatewayServer
/// [`WasmModbusClient`]: https://docs.rs/mbus-ffi
pub struct AsyncWsGatewayServer;

impl AsyncWsGatewayServer {
    // ── serve ─────────────────────────────────────────────────────────────────

    /// Bind and serve, running forever until an accept-loop error occurs.
    ///
    /// Each accepted WebSocket connection spawns an independent task.
    /// `router` and `downstreams` are wrapped in `Arc` and shared across all
    /// tasks.
    ///
    /// `downstreams` is a `Vec` where the index corresponds to the channel
    /// index returned by the routing policy.
    pub async fn serve<A, R, DS>(
        addr: A,
        config: WsGatewayConfig,
        router: R,
        downstreams: Vec<Arc<Mutex<DS>>>,
    ) -> Result<Infallible, AsyncGatewayError>
    where
        A: ToSocketAddrs,
        R: GatewayRoutingPolicy + Send + Sync + 'static,
        DS: AsyncTransport + Send + 'static,
    {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(AsyncGatewayError::BindFailed)?;

        let config = Arc::new(config);
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);
        let semaphore = make_semaphore(config.max_sessions);

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(AsyncGatewayError::AcceptFailed)?;

            let _ = stream.set_nodelay(true);
            let config_ref = config.clone();
            let router_ref = router.clone();
            let downstreams_ref = downstreams.clone();
            let sem_ref = semaphore.clone();

            gateway_log_debug!("incoming WS connection from {:?}", peer);

            tokio::spawn(async move {
                // ── Concurrency cap ───────────────────────────────────────────
                let _permit = match sem_ref {
                    Some(ref sem) => match sem.try_acquire() {
                        Ok(p) => Some(p),
                        Err(_) => {
                            gateway_log_warn!(
                                "session cap reached; rejecting connection from {:?}",
                                peer
                            );
                            // Drop `stream` — TCP RST is the cleanest fast-reject.
                            return;
                        }
                    },
                    None => None,
                };

                // ── WS handshake with header validation ───────────────────────
                let ws_stream = perform_handshake(stream, &config_ref).await;
                let ws_stream = match ws_stream {
                    Ok(ws) => ws,
                    Err(e) => {
                        gateway_log_debug!(
                            "WS handshake failed for {:?}: {:?}",
                            peer,
                            e
                        );
                        return;
                    }
                };

                // ── Run the session ───────────────────────────────────────────
                let upstream = WsUpstreamTransport::new(ws_stream);
                let result = match config_ref.idle_timeout {
                    Some(timeout) => {
                        let timed = IdleTimeoutTransport {
                            inner: upstream,
                            timeout,
                        };
                        run_async_session(timed, router_ref, downstreams_ref).await
                    }
                    None => {
                        run_async_session(upstream, router_ref, downstreams_ref).await
                    }
                };

                if let Err(e) = result {
                    gateway_log_debug!(
                        "WS session from {:?} ended with error: {:?}",
                        peer,
                        e
                    );
                }
            });
        }
    }

    // ── serve_with_shutdown ───────────────────────────────────────────────────

    /// Bind and serve until `shutdown` resolves.
    ///
    /// In-flight sessions continue to completion; only new connections stop
    /// being accepted after the shutdown signal fires.
    pub async fn serve_with_shutdown<A, R, DS, F>(
        addr: A,
        config: WsGatewayConfig,
        router: R,
        downstreams: Vec<Arc<Mutex<DS>>>,
        shutdown: F,
    ) -> Result<(), AsyncGatewayError>
    where
        A: ToSocketAddrs,
        R: GatewayRoutingPolicy + Send + Sync + 'static,
        DS: AsyncTransport + Send + 'static,
        F: Future<Output = ()>,
    {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(AsyncGatewayError::BindFailed)?;

        let config = Arc::new(config);
        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);
        let semaphore = make_semaphore(config.max_sessions);

        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, peer) = result.map_err(AsyncGatewayError::AcceptFailed)?;
                    let _ = stream.set_nodelay(true);
                    let config_ref = config.clone();
                    let router_ref = router.clone();
                    let downstreams_ref = downstreams.clone();
                    let sem_ref = semaphore.clone();

                    gateway_log_debug!("incoming WS connection from {:?}", peer);

                    tokio::spawn(async move {
                        let _permit = match sem_ref {
                            Some(ref sem) => match sem.try_acquire() {
                                Ok(p) => Some(p),
                                Err(_) => {
                                    gateway_log_warn!(
                                        "session cap reached; rejecting connection from {:?}",
                                        peer
                                    );
                                    return;
                                }
                            },
                            None => None,
                        };

                        let ws_stream = perform_handshake(stream, &config_ref).await;
                        let ws_stream = match ws_stream {
                            Ok(ws) => ws,
                            Err(e) => {
                                gateway_log_debug!(
                                    "WS handshake failed for {:?}: {:?}",
                                    peer,
                                    e
                                );
                                return;
                            }
                        };

                        let upstream = WsUpstreamTransport::new(ws_stream);
                        let result = match config_ref.idle_timeout {
                            Some(timeout) => {
                                let timed = IdleTimeoutTransport {
                                    inner: upstream,
                                    timeout,
                                };
                                run_async_session(timed, router_ref, downstreams_ref).await
                            }
                            None => {
                                run_async_session(upstream, router_ref, downstreams_ref).await
                            }
                        };

                        if let Err(e) = result {
                            gateway_log_debug!(
                                "WS session from {:?} ended with error: {:?}",
                                peer,
                                e
                            );
                        }
                    });
                }
                _ = &mut shutdown => {
                    gateway_log_debug!("WS gateway shutdown signal received; stopping accept loop");
                    return Ok(());
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Return `Some(Arc<Semaphore>)` when a session cap > 0 is configured.
fn make_semaphore(max_sessions: usize) -> Option<Arc<Semaphore>> {
    if max_sessions > 0 {
        Some(Arc::new(Semaphore::new(max_sessions)))
    } else {
        None
    }
}

/// Perform the WebSocket upgrade handshake, validating the `Origin` header and
/// the `Sec-WebSocket-Protocol` header according to `config`.
///
/// Returns the negotiated [`WebSocketStream`] on success, or a
/// `tungstenite::Error` on any failure (the error is always non-fatal; the
/// accept loop logs it and continues).
async fn perform_handshake(
    stream: tokio::net::TcpStream,
    config: &WsGatewayConfig,
) -> Result<
    tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    tokio_tungstenite::tungstenite::Error,
> {
    use tokio_tungstenite::tungstenite::handshake::server::{
        ErrorResponse, Request, Response,
    };
    use tokio_tungstenite::tungstenite::http::{HeaderValue, StatusCode};

    // Clone config data needed inside the FnOnce callback.
    let allowed_origins = config.allowed_origins.clone();
    let require_subprotocol = config.require_modbus_subprotocol;

    let callback =
        move |req: &Request, mut response: Response| -> Result<Response, ErrorResponse> {
            // ── Origin check ──────────────────────────────────────────────────
            if !allowed_origins.is_empty() {
                let origin_ok = req
                    .headers()
                    .get("origin")
                    .or_else(|| req.headers().get("Origin"))
                    .and_then(|v: &HeaderValue| v.to_str().ok())
                    .map(|origin_str: &str| {
                        allowed_origins.iter().any(|o| o == origin_str)
                    })
                    .unwrap_or(false);

                if !origin_ok {
                    let mut err = ErrorResponse::new(Some("Origin not allowed".to_string()));
                    *err.status_mut() = StatusCode::FORBIDDEN;
                    return Err(err);
                }
            }

            // ── Subprotocol negotiation ───────────────────────────────────────
            if require_subprotocol {
                let has_modbus = req
                    .headers()
                    .get("sec-websocket-protocol")
                    .and_then(|v: &HeaderValue| v.to_str().ok())
                    .map(|s: &str| s.split(',').any(|p: &str| p.trim() == "modbus"))
                    .unwrap_or(false);

                if !has_modbus {
                    let mut err = ErrorResponse::new(Some(
                        "modbus subprotocol required".to_string(),
                    ));
                    *err.status_mut() = StatusCode::BAD_REQUEST;
                    return Err(err);
                }

                // Echo the subprotocol back to the client.
                response.headers_mut().insert(
                    "sec-websocket-protocol",
                    HeaderValue::from_static("modbus"),
                );
            }

            Ok(response)
        };

    tokio_tungstenite::accept_hdr_async(stream, callback).await
}
