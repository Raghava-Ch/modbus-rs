//! Synchronous, non-blocking, poll-driven gateway services.
//!
//! [`GatewayServices`] is the `no_std`-compatible core of the gateway. Call
//! [`GatewayServices::poll`] in a tight loop to drive all upstream and downstream
//! channels concurrently without blocking or spin-waiting.

use heapless::Vec;
use mbus_core::data_unit::common::{
    MAX_ADU_FRAME_LEN, Pdu, compile_adu_frame, decompile_adu_frame, derive_length_from_bytes,
};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, TransportType, UnitIdOrSlaveAddr};

use crate::common::downstream_channel::DownstreamChannel;
use crate::common::event::GatewayEventHandler;
use crate::common::log_compat::{gateway_log_debug, gateway_log_trace, gateway_log_warn};
use crate::common::router::GatewayRoutingPolicy;
use crate::common::txn_map::TxnMap;
use crate::gateway_sync::channel_state::ChannelState;
use crate::gateway_sync::pending_queue::{PendingQueue, PendingRequest};
use crate::gateway_sync::upstream_channel::UpstreamChannel;

/// Summary of what happened in one `poll()` call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollOutcome {
    /// Nothing to do this cycle.
    Idle,
    /// At least one frame was forwarded or responded to.
    Active,
    /// All upstream channels have disconnected — gateway should be torn down.
    AllUpstreamsDisconnected,
}

/// Synchronous, non-blocking, poll-driven Modbus gateway.
///
/// Bridges multiple upstream transports to multiple downstream channels.
/// Fully generic, memory-efficient, and `no_std`-safe.
///
/// # Type Parameters
///
/// * `UpstreamT` — upstream transport (e.g. [`GatewayUpstream`](crate::GatewayUpstream)).
/// * `DownstreamT` — downstream transport (e.g. `StdRtuTransport`).
/// * `ROUTER` — implements [`GatewayRoutingPolicy`].
/// * `EVENT` — implements [`GatewayEventHandler`].
/// * `N_UPSTREAM` — maximum number of concurrent upstream channels (default 1).
/// * `N_DOWNSTREAM` — maximum number of concurrent downstream channels (default 1).
/// * `TXN_SIZE` — maximum concurrent in-flight transactions (default 4).
/// * `N_PENDING` — pending request queue capacity (default 0, which drops requests on busy).
pub struct GatewayServices<
    UpstreamT: Transport,
    DownstreamT: Transport,
    ROUTER,
    EVENT,
    const N_UPSTREAM: usize = 1,
    const N_DOWNSTREAM: usize = 1,
    const TXN_SIZE: usize = 4,
    const N_PENDING: usize = 0,
> {
    upstreams: heapless::Vec<UpstreamChannel<UpstreamT>, N_UPSTREAM>,
    downstreams: heapless::Vec<DownstreamChannel<DownstreamT>, N_DOWNSTREAM>,
    router: ROUTER,
    event_handler: EVENT,
    txn_map: TxnMap<TXN_SIZE>,
    pending: PendingQueue<N_PENDING>,
    /// Downstream response timeout in milliseconds.
    response_timeout_ms: u64,
}

impl<
    UpstreamT,
    DownstreamT,
    ROUTER,
    EVENT,
    const N_UP: usize,
    const N_DS: usize,
    const TXN_SIZE: usize,
    const N_PEND: usize,
> GatewayServices<UpstreamT, DownstreamT, ROUTER, EVENT, N_UP, N_DS, TXN_SIZE, N_PEND>
where
    UpstreamT: Transport,
    DownstreamT: Transport,
    ROUTER: GatewayRoutingPolicy,
    EVENT: GatewayEventHandler,
{
    /// Create a new gateway with the given routing policy, event handler, and timeout.
    ///
    /// Call [`add_upstream`](Self::add_upstream) and [`add_downstream`](Self::add_downstream)
    /// before calling `poll()`.
    pub fn new(router: ROUTER, event_handler: EVENT, response_timeout_ms: u64) -> Self {
        Self {
            upstreams: heapless::Vec::new(),
            downstreams: heapless::Vec::new(),
            router,
            event_handler,
            txn_map: TxnMap::new(),
            pending: PendingQueue::new(),
            response_timeout_ms,
        }
    }

    /// Register an upstream channel.
    pub fn add_upstream(&mut self, transport: UpstreamT) -> Result<(), MbusError> {
        let session_id = self.upstreams.len() as u8;
        self.upstreams
            .push(UpstreamChannel::new(transport, session_id))
            .map_err(|_| MbusError::TooManyRequests)
    }

    /// Register a downstream channel.
    pub fn add_downstream(
        &mut self,
        channel: DownstreamChannel<DownstreamT>,
    ) -> Result<(), MbusError> {
        self.downstreams
            .push(channel)
            .map_err(|_| MbusError::TooManyRequests)
    }

    /// Return an immutable reference to the event handler.
    pub fn event_handler(&self) -> &EVENT {
        &self.event_handler
    }

    /// Return a mutable reference to the event handler.
    pub fn event_handler_mut(&mut self) -> &mut EVENT {
        &mut self.event_handler
    }

    /// Return the number of upstream channels currently registered.
    pub fn upstream_count(&self) -> usize {
        self.upstreams.len()
    }

    /// Return an immutable reference to an upstream channel.
    pub fn upstream(&self, idx: usize) -> Option<&UpstreamChannel<UpstreamT>> {
        self.upstreams.get(idx)
    }

    /// Return a mutable reference to an upstream channel.
    pub fn upstream_mut(&mut self, idx: usize) -> Option<&mut UpstreamChannel<UpstreamT>> {
        self.upstreams.get_mut(idx)
    }

    /// Return an immutable reference to a downstream channel.
    pub fn downstream(&self, idx: usize) -> Option<&DownstreamChannel<DownstreamT>> {
        self.downstreams.get(idx)
    }

    /// Return a mutable reference to a downstream channel.
    pub fn downstream_mut(&mut self, idx: usize) -> Option<&mut DownstreamChannel<DownstreamT>> {
        self.downstreams.get_mut(idx)
    }

    /// Number of downstream channels currently registered.
    pub fn downstream_count(&self) -> usize {
        self.downstreams.len()
    }

    /// Return an immutable reference to the routing policy.
    pub fn router(&self) -> &ROUTER {
        &self.router
    }

    /// Return a mutable reference to the routing policy.
    pub fn router_mut(&mut self) -> &mut ROUTER {
        &mut self.router
    }

    /// Drive one non-blocking poll cycle.
    ///
    /// This function advances the internal state machines of all channels. It is
    /// guaranteed to return in O(N_UPSTREAM + N_DOWNSTREAM) transport calls without blocking.
    ///
    /// Call this in a tight loop to process transactions as they arrive.
    pub fn poll(&mut self, now_ms: u64) -> PollOutcome
    where
        UpstreamT::Error: Into<MbusError>,
        DownstreamT::Error: Into<MbusError>,
    {
        let mut outcome = PollOutcome::Idle;

        // ── Phase 1: Drain Downstream Channels ───────────────────────────────
        for ds_idx in 0..self.downstreams.len() {
            let state = self.downstreams[ds_idx].state;
            if let ChannelState::AwaitingResponse {
                internal_txn,
                session_idx,
                deadline_ms,
                upstream_txn,
                unit,
                fc,
                upstream_type,
            } = state
            {
                let mut completed = false;
                let mut timed_out = false;
                let mut connection_error = None;

                if now_ms >= deadline_ms {
                    timed_out = true;
                } else {
                    match self.downstreams[ds_idx].transport.recv() {
                        Ok(bytes) => {
                            gateway_log_trace!(
                                "downstream channel {} rx: {} bytes",
                                ds_idx,
                                bytes.len()
                            );
                            if self.downstreams[ds_idx]
                                .rxbuf
                                .extend_from_slice(&bytes)
                                .is_err()
                            {
                                gateway_log_warn!(
                                    "downstream channel {} rx buffer overflow; clearing",
                                    ds_idx
                                );
                                self.downstreams[ds_idx].rxbuf.clear();
                            }
                        }
                        Err(e) => {
                            let err: MbusError = e.into();
                            if !matches!(err, MbusError::Timeout) {
                                gateway_log_debug!(
                                    "downstream channel {} recv error: {:?}",
                                    ds_idx,
                                    err
                                );
                                connection_error = Some(err);
                            }
                        }
                    }

                    if connection_error.is_none() {
                        let ds_type = DownstreamT::TRANSPORT_TYPE;
                        if derive_length_from_bytes(&self.downstreams[ds_idx].rxbuf, ds_type)
                            .is_some_and(|expected_len| self.downstreams[ds_idx].rxbuf.len() >= expected_len)
                        {
                            completed = true;
                        }
                    }
                }

                if completed {
                    let ds_type = DownstreamT::TRANSPORT_TYPE;
                    let expected_len =
                        derive_length_from_bytes(&self.downstreams[ds_idx].rxbuf, ds_type).unwrap();

                    #[cfg(feature = "traffic")]
                    self.event_handler.on_downstream_rx(
                        session_idx as u8,
                        ds_idx,
                        &self.downstreams[ds_idx].rxbuf[..expected_len],
                    );

                    let response_msg_result = decompile_adu_frame(
                        &self.downstreams[ds_idx].rxbuf[..expected_len],
                        ds_type,
                    );

                    // Drain from rxbuf
                    let buf_len = self.downstreams[ds_idx].rxbuf.len();
                    if expected_len < buf_len {
                        self.downstreams[ds_idx]
                            .rxbuf
                            .copy_within(expected_len.., 0);
                    }
                    self.downstreams[ds_idx]
                        .rxbuf
                        .truncate(buf_len - expected_len);

                    if let (Ok(response_msg), Some(entry)) = (response_msg_result, self.txn_map.remove(internal_txn)) {
                        let adu_result = compile_adu_frame(
                            entry.upstream_txn,
                            unit.get(),
                            response_msg.pdu.clone(),
                            upstream_type,
                        );
                        if let (Ok(upstream_response_adu), Some(up_channel)) = (
                            adu_result,
                            self.upstreams
                                .iter_mut()
                                .find(|up| up.session_id as usize == session_idx),
                        ) {
                            if let Err(e) = up_channel.transport.send(&upstream_response_adu) {
                                let err: MbusError = e.into();
                                gateway_log_debug!("upstream send error: {:?}", err);
                            } else {
                                gateway_log_trace!(
                                    "upstream response sent: txn_id={}, unit={}",
                                    entry.upstream_txn,
                                    unit.get()
                                );
                                self.event_handler.on_response_returned(
                                    entry.session_id,
                                    entry.upstream_txn,
                                );
                                outcome = PollOutcome::Active;
                            }
                        }
                    }

                    self.downstreams[ds_idx].state = ChannelState::Idle;
                    self.try_service_pending(now_ms);
                } else if timed_out || connection_error.is_some() {
                    gateway_log_debug!("downstream channel {} request timed out or failed", ds_idx);
                    self.event_handler
                        .on_downstream_timeout(session_idx as u8, internal_txn);
                    let _ = self.txn_map.remove(internal_txn);

                    let _ = send_exception_upstream(
                        self.upstreams
                            .iter_mut()
                            .find(|up| up.session_id as usize == session_idx)
                            .map(|c| &mut c.transport),
                        upstream_txn,
                        unit,
                        fc,
                        ExceptionCode::GatewayPathUnavailable,
                        upstream_type,
                    );

                    self.downstreams[ds_idx].rxbuf.clear();
                    self.downstreams[ds_idx].state = ChannelState::Idle;
                    outcome = PollOutcome::Active;

                    self.try_service_pending(now_ms);
                }
            }
        }

        // ── Phase 2: Drain Upstream Channels ─────────────────────────────────
        let mut disconnected_sessions = heapless::Vec::<usize, N_UP>::new();

        for up_idx in 0..self.upstreams.len() {
            let session_idx = self.upstreams[up_idx].session_id as usize;
            match self.upstreams[up_idx].transport.recv() {
                Ok(bytes) => {
                    gateway_log_trace!(
                        "upstream session {} rx: {} bytes",
                        session_idx,
                        bytes.len()
                    );
                    if self.upstreams[up_idx]
                        .rxbuf
                        .extend_from_slice(&bytes)
                        .is_err()
                    {
                        gateway_log_warn!(
                            "upstream session {} rx buffer overflow; clearing",
                            session_idx
                        );
                        self.upstreams[up_idx].rxbuf.clear();
                    }
                }
                Err(e) => {
                    let err: MbusError = e.into();
                    match err {
                        MbusError::Timeout => {}
                        MbusError::ConnectionClosed
                        | MbusError::ConnectionLost
                        | MbusError::IoError => {
                            gateway_log_debug!(
                                "upstream session {} disconnected: {:?}",
                                session_idx,
                                err
                            );
                            disconnected_sessions.push(session_idx).ok();
                        }
                        _ => {
                            gateway_log_debug!(
                                "upstream session {} recv error: {:?}",
                                session_idx,
                                err
                            );
                        }
                    }
                }
            }

            let up_type = self.upstreams[up_idx].transport.transport_type_rt();
            while let Some(expected_len) =
                derive_length_from_bytes(&self.upstreams[up_idx].rxbuf, up_type)
            {
                if self.upstreams[up_idx].rxbuf.len() < expected_len {
                    break;
                }

                let mut raw_frame = Vec::<u8, MAX_ADU_FRAME_LEN>::new();
                let _ = raw_frame.extend_from_slice(&self.upstreams[up_idx].rxbuf[..expected_len]);

                // Drain from rxbuf
                let buf_len = self.upstreams[up_idx].rxbuf.len();
                if expected_len < buf_len {
                    self.upstreams[up_idx].rxbuf.copy_within(expected_len.., 0);
                }
                self.upstreams[up_idx]
                    .rxbuf
                    .truncate(buf_len - expected_len);

                if let Ok(upstream_msg) = decompile_adu_frame(&raw_frame, up_type) {
                    let unit = upstream_msg.unit_id_or_slave_addr();
                    let upstream_txn = upstream_msg.transaction_id();
                    let fc = upstream_msg.pdu.function_code();

                    gateway_log_trace!(
                        "upstream request from session {}: txn_id={}, unit={}, fc={:?}",
                        session_idx,
                        upstream_txn,
                        unit.get(),
                        fc
                    );

                    #[cfg(feature = "traffic")]
                    self.event_handler
                        .on_upstream_rx(session_idx as u8, &raw_frame);

                    let channel_idx = match self.router.route(unit) {
                        Some(idx) => idx,
                        None => {
                            gateway_log_debug!("routing miss: unit={}, no route", unit.get());
                            self.event_handler.on_routing_miss(session_idx as u8, unit);
                            let _ = send_exception_upstream(
                                Some(&mut self.upstreams[up_idx].transport),
                                upstream_txn,
                                unit,
                                fc,
                                ExceptionCode::GatewayPathUnavailable,
                                up_type,
                            );
                            outcome = PollOutcome::Active;
                            continue;
                        }
                    };

                    if channel_idx >= self.downstreams.len() {
                        gateway_log_warn!(
                            "routing policy returned channel_idx={} but downstreams count is {}",
                            channel_idx,
                            self.downstreams.len()
                        );
                        let _ = send_exception_upstream(
                            Some(&mut self.upstreams[up_idx].transport),
                            upstream_txn,
                            unit,
                            fc,
                            ExceptionCode::GatewayPathUnavailable,
                            up_type,
                        );
                        outcome = PollOutcome::Active;
                        continue;
                    }

                    let downstream_unit = self.router.rewrite(unit);

                    let req = PendingRequest {
                        session_idx,
                        upstream_txn,
                        unit,
                        downstream_unit,
                        fc,
                        pdu: upstream_msg.pdu.clone(),
                        upstream_type: up_type,
                    };

                    self.route_or_queue_request(req, now_ms);
                    outcome = PollOutcome::Active;
                }
            }
        }

        // ── Phase 3: Cleanup Disconnected Upstream Sessions ──────────────────
        for &session_idx in disconnected_sessions.iter() {
            if let Some(pos) = self
                .upstreams
                .iter()
                .position(|up| up.session_id as usize == session_idx)
            {
                self.upstreams.swap_remove(pos);
            }
            self.txn_map.remove_by_session(session_idx as u8);
            self.event_handler.on_upstream_disconnect(session_idx as u8);
            outcome = PollOutcome::Active;
        }

        if self.upstreams.is_empty() && !disconnected_sessions.is_empty() {
            PollOutcome::AllUpstreamsDisconnected
        } else {
            outcome
        }
    }

    fn route_or_queue_request(&mut self, req: PendingRequest, now_ms: u64) {
        if !self.pending.is_empty() {
            if self.pending.push(req.clone()) {
                gateway_log_trace!("downstream busy; request queued in pending queue");
                self.event_handler
                    .on_downstream_busy(req.session_idx as u8, req.unit, true);
            } else {
                gateway_log_warn!("pending queue full; dropping request");
                self.event_handler
                    .on_downstream_busy(req.session_idx as u8, req.unit, false);
                let _ = send_exception_upstream(
                    self.upstreams
                        .iter_mut()
                        .find(|up| up.session_id as usize == req.session_idx)
                        .map(|c| &mut c.transport),
                    req.upstream_txn,
                    req.unit,
                    req.fc,
                    ExceptionCode::GatewayTargetDeviceFailedToRespond,
                    req.upstream_type,
                );
            }
            return;
        }

        let channel_idx = self.router.route(req.unit).unwrap();

        if self.downstreams[channel_idx].state.is_idle() {
            self.dispatch_to_downstream(channel_idx, req, now_ms);
        } else if self.pending.push(req.clone()) {
            gateway_log_trace!("downstream busy; request queued in pending queue");
            self.event_handler
                .on_downstream_busy(req.session_idx as u8, req.unit, true);
        } else {
            gateway_log_warn!("pending queue disabled/full; dropping request");
            self.event_handler
                .on_downstream_busy(req.session_idx as u8, req.unit, false);
            let _ = send_exception_upstream(
                self.upstreams
                    .iter_mut()
                    .find(|up| up.session_id as usize == req.session_idx)
                    .map(|c| &mut c.transport),
                req.upstream_txn,
                req.unit,
                req.fc,
                ExceptionCode::GatewayTargetDeviceFailedToRespond,
                req.upstream_type,
            );
        }
    }

    fn dispatch_to_downstream(&mut self, channel_idx: usize, req: PendingRequest, now_ms: u64) {
        let internal_txn = match self
            .txn_map
            .allocate(req.upstream_txn, req.session_idx as u8)
        {
            Some(id) => id,
            None => {
                gateway_log_warn!("txn_map full; dropping request");
                let _ = send_exception_upstream(
                    self.upstreams
                        .iter_mut()
                        .find(|up| up.session_id as usize == req.session_idx)
                        .map(|c| &mut c.transport),
                    req.upstream_txn,
                    req.unit,
                    req.fc,
                    ExceptionCode::GatewayTargetDeviceFailedToRespond,
                    req.upstream_type,
                );
                return;
            }
        };

        let ds_type = DownstreamT::TRANSPORT_TYPE;
        let downstream_adu = match compile_adu_frame(
            internal_txn,
            req.downstream_unit.get(),
            req.pdu.clone(),
            ds_type,
        ) {
            Ok(adu) => adu,
            Err(e) => {
                gateway_log_debug!("failed to encode downstream ADU: {:?}", e);
                let _ = self.txn_map.remove(internal_txn);
                return;
            }
        };

        if let Err(e) = self.downstreams[channel_idx]
            .transport
            .send(&downstream_adu)
        {
            let err: MbusError = e.into();
            gateway_log_debug!("downstream channel {} send failed: {:?}", channel_idx, err);
            let _ = self.txn_map.remove(internal_txn);
            let _ = send_exception_upstream(
                self.upstreams
                    .iter_mut()
                    .find(|up| up.session_id as usize == req.session_idx)
                    .map(|c| &mut c.transport),
                req.upstream_txn,
                req.unit,
                req.fc,
                ExceptionCode::GatewayPathUnavailable,
                req.upstream_type,
            );
            return;
        }

        gateway_log_trace!("forwarded request to downstream channel {}", channel_idx);
        self.event_handler
            .on_forward(req.session_idx as u8, req.unit, channel_idx);

        #[cfg(feature = "traffic")]
        self.event_handler
            .on_downstream_tx(channel_idx, &downstream_adu);

        self.downstreams[channel_idx].state = ChannelState::AwaitingResponse {
            internal_txn,
            session_idx: req.session_idx,
            deadline_ms: now_ms + self.response_timeout_ms,
            upstream_txn: req.upstream_txn,
            unit: req.unit,
            fc: req.fc,
            upstream_type: req.upstream_type,
        };
    }

    fn try_service_pending(&mut self, now_ms: u64) {
        let mut limit = N_PEND;
        while limit > 0 && !self.pending.is_empty() {
            if let Some(req) = self.pending.pop_front() {
                let channel_idx = match self.router.route(req.unit) {
                    Some(idx) if idx < self.downstreams.len() => idx,
                    _ => {
                        let _ = send_exception_upstream(
                            self.upstreams
                                .iter_mut()
                                .find(|up| up.session_id as usize == req.session_idx)
                                .map(|c| &mut c.transport),
                            req.upstream_txn,
                            req.unit,
                            req.fc,
                            ExceptionCode::GatewayPathUnavailable,
                            req.upstream_type,
                        );
                        limit -= 1;
                        continue;
                    }
                };

                if self.downstreams[channel_idx].state.is_idle() {
                    self.dispatch_to_downstream(channel_idx, req, now_ms);
                } else {
                    let _ = self.pending.push(req);
                }
            }
            limit -= 1;
        }
    }
}

// ── Helper: send Modbus exception response upstream ──────────────────────────

fn send_exception_upstream<T: Transport>(
    upstream: Option<&mut T>,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    fc: FunctionCode,
    exception_code: ExceptionCode,
    transport_type: TransportType,
) -> Result<(), MbusError>
where
    T::Error: Into<MbusError>,
{
    let upstream = match upstream {
        Some(u) => u,
        None => return Ok(()),
    };
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
