//! Async Tokio-backed gateway server.
//!
//! [`AsyncTcpGatewayServer`] binds a TCP port, accepts upstream clients, and
//! spawns a per-session tokio task for each one.  Downstream channels are
//! shared between sessions via `Arc<tokio::sync::Mutex<T>>`.

use std::convert::Infallible;
use std::future::Future;
use std::sync::Arc;

use mbus_core::data_unit::common::{
    compile_adu_frame, decompile_adu_frame, Pdu,
};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{AsyncTransport, TransportType, UnitIdOrSlaveAddr};
use mbus_network::TokioTcpTransport;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::Mutex;

use crate::log_compat::{gateway_log_debug, gateway_log_trace, gateway_log_warn};
use crate::router::GatewayRoutingPolicy;

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

/// Errors returned by [`AsyncTcpGatewayServer`].
#[derive(Debug)]
pub enum AsyncGatewayError {
    /// The gateway could not bind to the requested address.
    BindFailed(std::io::Error),
    /// An error occurred in the accept loop that prevents further operation.
    AcceptFailed(std::io::Error),
    /// A Modbus protocol-level error.
    Modbus(MbusError),
}

impl core::fmt::Display for AsyncGatewayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            AsyncGatewayError::BindFailed(e) => write!(f, "gateway bind failed: {e}"),
            AsyncGatewayError::AcceptFailed(e) => write!(f, "gateway accept failed: {e}"),
            AsyncGatewayError::Modbus(e) => write!(f, "gateway modbus error: {e:?}"),
        }
    }
}

impl std::error::Error for AsyncGatewayError {}

impl From<MbusError> for AsyncGatewayError {
    fn from(e: MbusError) -> Self {
        AsyncGatewayError::Modbus(e)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// AsyncTcpGatewayServer
// ─────────────────────────────────────────────────────────────────────────────

/// Async Modbus TCP gateway.
///
/// Accepts TCP connections from upstream Modbus clients and forwards their
/// requests to downstream channels.  Each upstream connection is handled in a
/// dedicated `tokio::spawn`-ed task.
///
/// # Downstream channels
///
/// All downstream channels must be the same transport type `DS`.  To use
/// different transport types on different buses, wrap them in a custom enum
/// that implements [`AsyncTransport`].
///
/// # Example
///
/// ```rust,no_run
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// use mbus_gateway::{AsyncTcpGatewayServer, UnitRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
/// use mbus_network::TokioTcpTransport;
/// use std::sync::Arc;
/// use tokio::sync::Mutex;
///
/// // Route unit 1 to channel 0 and unit 2 to channel 1.
/// let mut router: UnitRouteTable<4> = UnitRouteTable::new();
/// router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
/// router.add(UnitIdOrSlaveAddr::new(2).unwrap(), 0).unwrap();
///
/// let downstream = TokioTcpTransport::connect("192.168.1.10:502").await?;
/// let shared = Arc::new(Mutex::new(downstream));
///
/// AsyncTcpGatewayServer::serve("0.0.0.0:502", router, vec![shared]).await?;
/// # Ok(())
/// # }
/// ```
pub struct AsyncTcpGatewayServer;

impl AsyncTcpGatewayServer {
    // ── serve ─────────────────────────────────────────────────────────────────

    /// Bind and serve, running forever until an accept-loop error occurs.
    ///
    /// Each accepted connection spawns an independent task.  `router` and
    /// `downstreams` are wrapped in `Arc` and shared across all tasks.
    ///
    /// `downstreams` is a `Vec` where the index corresponds to the channel index
    /// returned by the routing policy.
    pub async fn serve<A, R, DS>(
        addr: A,
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

        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);

        loop {
            let (stream, peer) = listener
                .accept()
                .await
                .map_err(AsyncGatewayError::AcceptFailed)?;

            let _ = stream.set_nodelay(true);
            let upstream = TokioTcpTransport::from_stream(stream);
            let router_ref = router.clone();
            let downstreams_ref = downstreams.clone();

            gateway_log_debug!("accepted upstream connection from {:?}", peer);

            tokio::spawn(async move {
                if let Err(e) =
                    run_async_session(upstream, router_ref, downstreams_ref).await
                {
                    gateway_log_debug!("session ended: {:?}", e);
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

        let router = Arc::new(router);
        let downstreams = Arc::new(downstreams);

        tokio::pin!(shutdown);

        loop {
            tokio::select! {
                result = listener.accept() => {
                    let (stream, peer) = result.map_err(AsyncGatewayError::AcceptFailed)?;
                    let _ = stream.set_nodelay(true);
                    let upstream = TokioTcpTransport::from_stream(stream);
                    let router_ref = router.clone();
                    let downstreams_ref = downstreams.clone();

                    gateway_log_debug!("accepted upstream connection from {:?}", peer);

                    tokio::spawn(async move {
                        if let Err(e) =
                            run_async_session(upstream, router_ref, downstreams_ref).await
                        {
                            gateway_log_debug!("session ended: {:?}", e);
                        }
                    });
                }
                _ = &mut shutdown => {
                    gateway_log_debug!("shutdown signal received; stopping accept loop");
                    return Ok(());
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-session task
// ─────────────────────────────────────────────────────────────────────────────

/// Inner async per-session loop.
///
/// Receives upstream frames, routes them, forwards to a downstream channel
/// (holding the mutex for the duration of the downstream request-response
/// round trip), and sends the response back upstream.
async fn run_async_session<UPSTREAM, ROUTER, DS>(
    mut upstream: UPSTREAM,
    router: Arc<ROUTER>,
    downstreams: Arc<Vec<Arc<Mutex<DS>>>>,
) -> Result<(), MbusError>
where
    UPSTREAM: AsyncTransport,
    ROUTER: GatewayRoutingPolicy + Send + Sync,
    DS: AsyncTransport + Send,
{
    let upstream_type = UPSTREAM::TRANSPORT_TYPE;
    // Per-session monotonic transaction counter used for the downstream.
    let mut next_txn: u16 = 0;

    loop {
        // ── Receive one complete upstream frame ────────────────────────────
        let frame = match upstream.recv().await {
            Ok(f) => f,
            Err(MbusError::ConnectionClosed) | Err(MbusError::ConnectionLost) => {
                gateway_log_debug!("upstream disconnected");
                break;
            }
            Err(e) => {
                gateway_log_debug!("upstream recv error: {:?}", e);
                break;
            }
        };

        gateway_log_trace!(
            "upstream rx: {} bytes (type={:?})",
            frame.len(),
            upstream_type
        );

        // ── Parse upstream ADU ─────────────────────────────────────────────
        let msg = match decompile_adu_frame(&frame, upstream_type) {
            Ok(m) => m,
            Err(e) => {
                gateway_log_debug!("upstream frame parse error: {:?}", e);
                continue;
            }
        };

        let unit = msg.unit_id_or_slave_addr();
        let upstream_txn = msg.transaction_id();
        let fc = msg.pdu.function_code();

        gateway_log_trace!(
            "upstream frame: txn={}, unit={}, fc=0x{:02X}",
            upstream_txn,
            unit.get(),
            fc as u8
        );

        // ── Route by unit ID ───────────────────────────────────────────────
        let channel_idx = match router.route(unit) {
            Some(idx) => idx,
            None => {
                gateway_log_debug!("routing miss for unit={}", unit.get());
                let _ = send_async_exception(
                    &mut upstream,
                    upstream_txn,
                    unit,
                    fc,
                    ExceptionCode::ServerDeviceFailure,
                    upstream_type,
                )
                .await;
                continue;
            }
        };

        if channel_idx >= downstreams.len() {
            gateway_log_warn!(
                "routing policy returned channel_idx={} but only {} channel(s) available",
                channel_idx,
                downstreams.len()
            );
            let _ = send_async_exception(
                &mut upstream,
                upstream_txn,
                unit,
                fc,
                ExceptionCode::ServerDeviceFailure,
                upstream_type,
            )
            .await;
            continue;
        }

        // ── Allocate internal txn and re-encode for downstream ─────────────
        let internal_txn = next_txn;
        next_txn = next_txn.wrapping_add(1);

        // Apply optional unit-ID rewrite from the routing policy.
        let downstream_unit = router.rewrite(unit);

        let downstream_type = DS::TRANSPORT_TYPE;
        let ds_adu = match compile_adu_frame(internal_txn, downstream_unit.get(), msg.pdu.clone(), downstream_type) {
            Ok(adu) => adu,
            Err(e) => {
                gateway_log_debug!("failed to encode downstream ADU: {:?}", e);
                continue;
            }
        };

        gateway_log_trace!(
            "forwarding to downstream channel {}: {} bytes",
            channel_idx,
            ds_adu.len()
        );

        // ── Lock downstream, send, wait for response ───────────────────────
        let response_bytes = {
            let mut ds = downstreams[channel_idx].lock().await;

            if let Err(e) = ds.send(&ds_adu).await {
                gateway_log_debug!("downstream send error: {:?}", e);
                continue;
            }

            match ds.recv().await {
                Ok(b) => b,
                Err(e) => {
                    gateway_log_debug!("downstream recv error: {:?}", e);
                    continue;
                }
            }
        };

        // ── Parse downstream response ─────────────────────────────────────
        let response_msg = match decompile_adu_frame(&response_bytes, downstream_type) {
            Ok(m) => m,
            Err(e) => {
                gateway_log_debug!("downstream response parse error: {:?}", e);
                continue;
            }
        };

        // ── Re-encode for upstream ─────────────────────────────────────────
        let us_adu = match compile_adu_frame(
            upstream_txn,
            unit.get(),
            response_msg.pdu.clone(),
            upstream_type,
        ) {
            Ok(adu) => adu,
            Err(e) => {
                gateway_log_debug!("failed to encode upstream response: {:?}", e);
                continue;
            }
        };

        gateway_log_trace!(
            "sending upstream response: txn={}, {} bytes",
            upstream_txn,
            us_adu.len()
        );

        if let Err(e) = upstream.send(&us_adu).await {
            gateway_log_debug!("upstream send error: {:?}", e);
            break;
        }
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: send async Modbus exception response
// ─────────────────────────────────────────────────────────────────────────────

async fn send_async_exception<T: AsyncTransport>(
    upstream: &mut T,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    exception_code: ExceptionCode,
    transport_type: TransportType,
) -> Result<(), MbusError> {
    let exception_fc = match fc.exception_response() {
        Some(efc) => efc,
        None => return Ok(()),
    };
    let pdu =
        Pdu::build_byte_payload(exception_fc, exception_code as u8).map_err(|_| MbusError::Unexpected)?;
    let adu = compile_adu_frame(txn_id, unit.get(), pdu, transport_type)?;
    upstream.send(&adu).await
}
