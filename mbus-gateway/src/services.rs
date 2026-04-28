//! Synchronous, poll-driven gateway services.
//!
//! [`GatewayServices`] is the no_std-compatible core of the gateway.  Call
//! [`GatewayServices::poll`] in a tight loop to process one request-response
//! cycle at a time.
//!
//! ## Lifecycle
//!
//! 1. Construct with [`GatewayServices::new`].
//! 2. Register downstream channels with [`add_downstream`](GatewayServices::add_downstream).
//! 3. (Optional) call `upstream.connect()` if the upstream transport requires it.
//! 4. Loop: call `poll()`.

use heapless::Vec;
use mbus_core::data_unit::common::{
    compile_adu_frame, decompile_adu_frame, derive_length_from_bytes, ModbusMessage,
    Pdu, MAX_ADU_FRAME_LEN,
};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, TransportType, UnitIdOrSlaveAddr};

use crate::dispatcher::DownstreamChannel;
use crate::event::GatewayEventHandler;
use crate::log_compat::{gateway_log_debug, gateway_log_trace, gateway_log_warn};
use crate::router::GatewayRoutingPolicy;
use crate::txn_map::TxnMap;

// ─────────────────────────────────────────────────────────────────────────────
// GatewayServices
// ─────────────────────────────────────────────────────────────────────────────

/// Synchronous, poll-driven Modbus gateway.
///
/// Bridges a single upstream transport to up to `N_DOWNSTREAM` downstream
/// channels of the same transport type.  Use the `async` feature for a
/// multi-session async gateway.
///
/// # Type Parameters
///
/// * `UpstreamT` — upstream transport (e.g. `StdTcpServerTransport`).
/// * `DownstreamT` — downstream transport (e.g. `StdTcpTransport` or `StdRtuTransport`).
/// * `ROUTER` — implements [`GatewayRoutingPolicy`].
/// * `EVENT` — implements [`GatewayEventHandler`].
/// * `N_DOWNSTREAM` — maximum number of downstream channels (const generic, default 1).
/// * `TXN_SIZE` — maximum concurrent in-flight transactions (default 1 for sync).
///
/// # Example
///
/// ```rust,no_run
/// use mbus_gateway::{GatewayServices, PassthroughRouter, NoopEventHandler, DownstreamChannel};
/// // Supply your own transport implementations.
/// // let mut gw: GatewayServices<MyUpstream, MyDownstream, _, _, 1> =
/// //     GatewayServices::new(upstream, PassthroughRouter, NoopEventHandler);
/// // gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
/// // loop { let _ = gw.poll(); }
/// ```
pub struct GatewayServices<
    UpstreamT: Transport,
    DownstreamT: Transport,
    ROUTER,
    EVENT,
    const N_DOWNSTREAM: usize = 1,
    const TXN_SIZE: usize = 1,
> {
    upstream: UpstreamT,
    upstream_rxbuf: Vec<u8, MAX_ADU_FRAME_LEN>,
    downstream: heapless::Vec<DownstreamChannel<DownstreamT>, N_DOWNSTREAM>,
    router: ROUTER,
    event_handler: EVENT,
    txn_map: TxnMap<TXN_SIZE>,
    /// Maximum number of `recv()` attempts on the downstream transport before
    /// declaring a timeout.  The default (200) is appropriate for non-blocking
    /// transports; reduce to 1 for blocking transports with a configured
    /// read-timeout.
    max_downstream_recv_attempts: usize,
}

impl<UpstreamT, DownstreamT, ROUTER, EVENT, const N_DS: usize, const TXN_SIZE: usize>
    GatewayServices<UpstreamT, DownstreamT, ROUTER, EVENT, N_DS, TXN_SIZE>
where
    UpstreamT: Transport,
    DownstreamT: Transport,
    ROUTER: GatewayRoutingPolicy,
    EVENT: GatewayEventHandler,
{
    /// Create a new gateway with the given upstream transport, routing policy,
    /// and event handler.
    ///
    /// Call [`add_downstream`](Self::add_downstream) at least once before
    /// calling `poll()`.
    pub fn new(upstream: UpstreamT, router: ROUTER, event_handler: EVENT) -> Self {
        Self {
            upstream,
            upstream_rxbuf: Vec::new(),
            downstream: heapless::Vec::new(),
            router,
            event_handler,
            txn_map: TxnMap::new(),
            max_downstream_recv_attempts: 200,
        }
    }

    /// Register a downstream channel.
    ///
    /// Channels are indexed in registration order (0, 1, …).  Returns
    /// `Err(MbusError::TooManyRequests)` when `N_DOWNSTREAM` channels have
    /// already been registered.
    pub fn add_downstream(
        &mut self,
        channel: DownstreamChannel<DownstreamT>,
    ) -> Result<(), MbusError> {
        self.downstream
            .push(channel)
            .map_err(|_| MbusError::TooManyRequests)
    }

    /// Set the maximum number of `recv()` attempts on a downstream transport
    /// before the gateway declares a timeout.
    ///
    /// Default: 200.  Set to 1 for blocking transports that have a configured
    /// read-timeout (the transport's own timeout expires after the first call).
    pub fn set_max_downstream_recv_attempts(&mut self, attempts: usize) {
        self.max_downstream_recv_attempts = attempts;
    }

    /// Return an immutable reference to the event handler.
    pub fn event_handler(&self) -> &EVENT {
        &self.event_handler
    }

    /// Return a mutable reference to the event handler.
    pub fn event_handler_mut(&mut self) -> &mut EVENT {
        &mut self.event_handler
    }

    /// Return an immutable reference to the upstream transport.
    pub fn upstream(&self) -> &UpstreamT {
        &self.upstream
    }

    /// Return a mutable reference to the upstream transport.
    pub fn upstream_mut(&mut self) -> &mut UpstreamT {
        &mut self.upstream
    }

    /// Return an immutable reference to a downstream channel.
    pub fn downstream(&self, idx: usize) -> Option<&DownstreamChannel<DownstreamT>> {
        self.downstream.get(idx)
    }

    /// Return a mutable reference to a downstream channel.
    pub fn downstream_mut(
        &mut self,
        idx: usize,
    ) -> Option<&mut DownstreamChannel<DownstreamT>> {
        self.downstream.get_mut(idx)
    }

    /// Drive one poll cycle.
    ///
    /// Each call to `poll`:
    /// 1. Attempts a non-blocking `recv()` on the upstream transport.
    /// 2. Accumulates received bytes in the upstream receive buffer.
    /// 3. Tries to parse a complete upstream ADU frame.
    /// 4. Routes the request by unit ID via the routing policy.
    /// 5. Re-encodes the PDU for the downstream transport type and forwards it.
    /// 6. Waits (up to `max_downstream_recv_attempts` recv calls) for the
    ///    downstream response.
    /// 7. Re-encodes the response for the upstream transport type and sends it.
    ///
    /// # Return value
    ///
    /// * `Ok(())` — no upstream data was available, or the request-response
    ///   cycle completed successfully.
    /// * `Err(MbusError::ConnectionClosed)` — the upstream transport reported
    ///   a connection-level error.
    /// * `Err(MbusError::Timeout)` — the downstream device did not respond in
    ///   time.
    /// * Other `Err` variants are propagated from the transport layers.
    pub fn poll(&mut self) -> Result<(), MbusError>
    where
        UpstreamT::Error: Into<MbusError>,
        DownstreamT::Error: Into<MbusError>,
    {
        // ── Step 1: try to receive bytes from the upstream transport ───────────
        match self.upstream.recv() {
            Ok(bytes) => {
                gateway_log_trace!("upstream rx: {} bytes", bytes.len());
                if self.upstream_rxbuf.extend_from_slice(&bytes).is_err() {
                    // Buffer overflow — discard everything and wait for resync.
                    gateway_log_warn!(
                        "upstream rx buffer overflow ({} bytes); discarding",
                        self.upstream_rxbuf.len()
                    );
                    self.upstream_rxbuf.clear();
                    return Ok(());
                }
            }
            Err(e) => {
                let err: MbusError = e.into();
                match err {
                    MbusError::Timeout => {
                        // Normal: no data available on this poll cycle.
                        return Ok(());
                    }
                    MbusError::ConnectionClosed
                    | MbusError::ConnectionLost
                    | MbusError::IoError => {
                        gateway_log_debug!("upstream transport error: {:?}", err);
                        self.event_handler.on_upstream_disconnect(0);
                        self.upstream_rxbuf.clear();
                        return Err(err);
                    }
                    _ => {
                        gateway_log_debug!("upstream recv error: {:?}", err);
                        return Ok(());
                    }
                }
            }
        }

        // ── Step 2: try to extract a complete upstream frame ──────────────────
        let upstream_type = UpstreamT::TRANSPORT_TYPE;
        let expected_len =
            match derive_length_from_bytes(&self.upstream_rxbuf, upstream_type) {
                Some(len) => len,
                None => {
                    // Incomplete frame — wait for more bytes.
                    return Ok(());
                }
            };

        if self.upstream_rxbuf.len() < expected_len {
            return Ok(());
        }

        // Extract the raw frame bytes (we need them before parsing so we can
        // drain the buffer even if parsing fails).
        let raw_frame: Vec<u8, MAX_ADU_FRAME_LEN> = {
            let mut v: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
            let _ = v.extend_from_slice(&self.upstream_rxbuf[..expected_len]);
            v
        };

        // Drain the consumed bytes from the upstream rx buffer.
        let buf_len = self.upstream_rxbuf.len();
        if expected_len < buf_len {
            self.upstream_rxbuf.copy_within(expected_len.., 0);
        }
        self.upstream_rxbuf.truncate(buf_len - expected_len);

        // ── Step 3: parse the upstream ADU ────────────────────────────────────
        let upstream_msg = match decompile_adu_frame(&raw_frame, upstream_type) {
            Ok(msg) => msg,
            Err(e) => {
                gateway_log_debug!("upstream frame parse error: {:?}", e);
                return Ok(());
            }
        };

        let unit = upstream_msg.unit_id_or_slave_addr();
        let upstream_txn = upstream_msg.transaction_id();
        let fc = upstream_msg.pdu.function_code();

        gateway_log_trace!(
            "upstream frame: txn_id={}, unit={}, fc=0x{:02X}",
            upstream_txn,
            unit.get(),
            fc as u8
        );

        #[cfg(feature = "traffic")]
        self.event_handler.on_upstream_rx(0, &raw_frame);

        // ── Step 4: route by unit ID ──────────────────────────────────────────
        let channel_idx = match self.router.route(unit) {
            Some(idx) => idx,
            None => {
                gateway_log_debug!(
                    "routing miss: unit={}, no downstream channel configured",
                    unit.get()
                );
                self.event_handler.on_routing_miss(0, unit);
                // Send a Modbus exception back upstream so the client knows the
                // gateway could not find a route.
                let _ = send_exception_upstream(
                    &mut self.upstream,
                    upstream_txn,
                    unit,
                    fc,
                    ExceptionCode::ServerDeviceFailure,
                    upstream_type,
                );
                return Ok(());
            }
        };

        if channel_idx >= self.downstream.len() {
            gateway_log_warn!(
                "routing policy returned channel_idx={} but only {} channel(s) registered",
                channel_idx,
                self.downstream.len()
            );
            let _ = send_exception_upstream(
                &mut self.upstream,
                upstream_txn,
                unit,
                fc,
                ExceptionCode::ServerDeviceFailure,
                upstream_type,
            );
            return Ok(());
        }

        self.event_handler.on_forward(0, unit, channel_idx);

        // ── Step 5: allocate internal txn id & re-encode for downstream ───────
        let internal_txn = match self.txn_map.allocate(upstream_txn, 0) {
            Some(id) => id,
            None => {
                gateway_log_warn!("txn_map full; dropping upstream request");
                return Ok(());
            }
        };

        // Apply optional unit-ID rewrite from the routing policy (e.g.
        // UnitIdRewriteRouter applies an additive offset here).
        let downstream_unit = self.router.rewrite(unit);

        let downstream_type = DownstreamT::TRANSPORT_TYPE;
        let downstream_adu =
            match compile_adu_frame(internal_txn, downstream_unit.get(), upstream_msg.pdu.clone(), downstream_type) {
                Ok(adu) => adu,
                Err(e) => {
                    gateway_log_debug!("failed to encode downstream ADU: {:?}", e);
                    let _ = self.txn_map.remove(internal_txn);
                    return Ok(());
                }
            };

        // ── Step 6: send to downstream ────────────────────────────────────────
        let max_attempts = self.max_downstream_recv_attempts;

        {
            let channel = &mut self.downstream[channel_idx];

            #[cfg(feature = "traffic")]
            {
                // We can't call self.event_handler here because self.downstream is
                // borrowed.  To fire the traffic callback we borrow the frame slice
                // temporarily.
                let _ = downstream_adu.as_slice(); // keep adu in scope
            }

            if let Err(e) = channel.transport.send(&downstream_adu) {
                let err: MbusError = e.into();
                gateway_log_debug!("downstream send error: {:?}", err);
                let _ = self.txn_map.remove(internal_txn);
                return Err(err);
            }

            gateway_log_trace!(
                "downstream tx: {} bytes to channel {}",
                downstream_adu.len(),
                channel_idx
            );
        }

        // Fire traffic callback after releasing the downstream borrow.
        #[cfg(feature = "traffic")]
        self.event_handler
            .on_downstream_tx(channel_idx, &downstream_adu);

        // ── Step 7: receive downstream response ───────────────────────────────
        let response_result = recv_downstream_response_blocking::<DownstreamT>(
            &mut self.downstream[channel_idx],
            downstream_type,
            max_attempts,
        );

        let response_msg = match response_result {
            Ok(msg) => msg,
            Err(e) => {
                gateway_log_debug!("downstream recv error: {:?}", e);
                self.event_handler.on_downstream_timeout(0, internal_txn);
                let _ = self.txn_map.remove(internal_txn);
                return Err(e);
            }
        };

        // ── Step 8: restore upstream txn id from map ──────────────────────────
        let entry = match self.txn_map.remove(internal_txn) {
            Some(e) => e,
            None => {
                gateway_log_debug!("txn map entry missing for internal_txn={}", internal_txn);
                return Ok(());
            }
        };

        // ── Step 9: re-encode for upstream ────────────────────────────────────
        let upstream_response_adu = match compile_adu_frame(
            entry.upstream_txn,
            unit.get(),
            response_msg.pdu.clone(),
            upstream_type,
        ) {
            Ok(adu) => adu,
            Err(e) => {
                gateway_log_debug!("failed to encode upstream response ADU: {:?}", e);
                return Ok(());
            }
        };

        // ── Step 10: send upstream response ───────────────────────────────────
        if let Err(e) = self.upstream.send(&upstream_response_adu) {
            let err: MbusError = e.into();
            gateway_log_debug!("upstream send error: {:?}", err);
            return Err(err);
        }

        gateway_log_trace!(
            "upstream response sent: txn_id={}, unit={}",
            entry.upstream_txn,
            unit.get()
        );

        self.event_handler.on_response_returned(0, entry.upstream_txn);

        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: blocking downstream receive
// ─────────────────────────────────────────────────────────────────────────────

/// Spin-receive on `channel` until a complete ADU frame is assembled.
///
/// For **TCP** transports (where `Timeout` means the read-timeout expired) this
/// returns `Err(MbusError::Timeout)` on the first timeout, which is the correct
/// behaviour since the device didn't respond within the configured period.
///
/// For **serial** transports (where `Timeout` may mean "no bytes yet") this
/// continues retrying up to `max_attempts` times before giving up.
fn recv_downstream_response_blocking<T: Transport>(
    channel: &mut DownstreamChannel<T>,
    transport_type: TransportType,
    max_attempts: usize,
) -> Result<ModbusMessage, MbusError>
where
    T::Error: Into<MbusError>,
{
    let is_serial = transport_type.is_serial_type();

    for attempt in 0..max_attempts {
        match channel.transport.recv() {
            Ok(bytes) => {
                gateway_log_trace!(
                    "downstream rx attempt {}: {} bytes",
                    attempt,
                    bytes.len()
                );
                if channel.rxbuf.extend_from_slice(&bytes).is_err() {
                    // Overflow — discard and try again.
                    channel.rxbuf.clear();
                    continue;
                }
            }
            Err(e) => {
                let err: MbusError = e.into();
                if matches!(err, MbusError::Timeout) && is_serial {
                    // Serial "no bytes yet" — keep polling.
                    continue;
                }
                // For TCP, Timeout means the read-deadline expired.
                return Err(err);
            }
        }

        // Try to extract a complete frame.
        if let Some(expected_len) = derive_length_from_bytes(&channel.rxbuf, transport_type) {
            if channel.rxbuf.len() >= expected_len {
                let msg = decompile_adu_frame(&channel.rxbuf[..expected_len], transport_type)?;

                // Drain the consumed bytes.
                let buf_len = channel.rxbuf.len();
                if expected_len < buf_len {
                    channel.rxbuf.copy_within(expected_len.., 0);
                }
                channel.rxbuf.truncate(buf_len - expected_len);

                return Ok(msg);
            }
        }
    }

    Err(MbusError::Timeout)
}

// ─────────────────────────────────────────────────────────────────────────────
// Helper: send Modbus exception response upstream
// ─────────────────────────────────────────────────────────────────────────────

/// Build and send a Modbus exception ADU to the upstream transport.
///
/// Failures are silently logged and ignored; the gateway still returns `Ok(())`
/// in the caller since the routing miss was handled.
fn send_exception_upstream<T: Transport>(
    upstream: &mut T,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    exception_code: ExceptionCode,
    transport_type: TransportType,
) -> Result<(), MbusError>
where
    T::Error: Into<MbusError>,
{
    let exception_fc = match fc.exception_response() {
        Some(efc) => efc,
        None => {
            gateway_log_debug!("FC 0x{:02X} has no exception variant", fc as u8);
            return Ok(());
        }
    };
    let pdu = Pdu::build_byte_payload(exception_fc, exception_code as u8)
        .map_err(|_| MbusError::Unexpected)?;
    let adu = compile_adu_frame(txn_id, unit.get(), pdu, transport_type)?;
    upstream.send(&adu).map_err(|e| e.into())
}
