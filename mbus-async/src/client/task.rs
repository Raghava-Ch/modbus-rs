//! Background async task that owns the transport and drives the Modbus loop.
//!
//! # Architecture
//!
//! ```text
//! AsyncTcpClient / AsyncSerialClient
//!   └── AsyncClientCore           (public API, holds cmd_tx + pending_count_rx)
//!         └── mpsc::Sender<TaskCommand>  ──────► ClientTask::run()
//!                                                  ├── ConnectFactory (creates transport on Connect)
//!                                                  ├── transport: Option<T>
//!                                                  ├── pending: HashMap<u16, PendingEntry>
//!                                                  └── tokio::select! { recv_frame | cmd }
//! ```
//!
//! # Pipelining
//!
//! The `N` parameter controls how many requests may be in-flight simultaneously
//! over the same connection.  For TCP, `N = 9` is a sensible default; for serial
//! connections `N` should be `1` (each request waits for its response before the
//! next is sent).
//!
//! Requests that arrive when `in_flight >= N` are queued in `self.queued` and
//! drained back into the pipeline as responses arrive.
//!
//! # Serial matching
//!
//! Serial frames carry no MBAP transaction id.  `decompile_adu_frame` always
//! returns `txn_id = 0` for RTU/ASCII frames.  When this happens the task
//! matches the response against the *first* (and, for `N = 1`, only) entry in
//! the pending map.

use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;

use heapless::Vec as HVec;
use tokio::sync::{mpsc, watch};

use mbus_core::{
    data_unit::common::MAX_ADU_FRAME_LEN, errors::MbusError, transport::AsyncTransport,
};

use crate::client::command::{ClientRequest, ResponseSender, TaskCommand};
use crate::client::decode::decode_response;
use crate::client::encode::encode_request;

#[cfg(feature = "traffic")]
use crate::client::notifier::NotifierStore;

// ─── Public type aliases ──────────────────────────────────────────────────────

/// Boxed factory that produces a new, fully-connected transport on demand.
///
/// Used both for the initial connection and for reconnections.  The factory
/// owns whatever addresses/configs are needed to open the transport so the
/// task itself does not need transport-specific knowledge.
///
/// For TCP:
/// ```rust,ignore
/// let f: ConnectFactory<TokioTcpTransport> = Box::new(move || {
///     Box::pin(TokioTcpTransport::connect((host.clone(), port)))
/// });
/// ```
pub(crate) type ConnectFactory<T> =
    Box<dyn Fn() -> Pin<Box<dyn Future<Output = Result<T, MbusError>> + Send>> + Send + 'static>;

/// Receiver half of the watch channel that carries the current pending-request
/// count.  Held by [`AsyncClientCore`] for a synchronous `has_pending_requests()`.
///
/// [`AsyncClientCore`]: crate::client::client_core::AsyncClientCore
pub(crate) type PendingCountReceiver = watch::Receiver<usize>;

// ─── Internal types ───────────────────────────────────────────────────────────

/// One entry in the in-flight map.
struct PendingEntry {
    /// Channel to deliver the result to the waiting caller.
    resp_tx: ResponseSender,
    /// Original request parameters — used for traffic hooks and response fix-up.
    request: ClientRequest,
}

// ─── ClientTask ───────────────────────────────────────────────────────────────

/// Background async task that drives Modbus communication for one logical connection.
///
/// Spawned by transport-specific constructors and driven via `tokio::spawn(task.run())`.
pub(crate) struct ClientTask<T, const N: usize = 9>
where
    T: AsyncTransport + Send + 'static,
{
    /// Currently active transport, or `None` when not yet connected / after an error.
    transport: Option<T>,
    /// Factory that creates a fresh connected transport.
    connect_fn: ConnectFactory<T>,
    /// Receives commands from the public API (`AsyncClientCore`).
    cmd_rx: mpsc::Receiver<TaskCommand>,
    /// In-flight requests keyed by the task-assigned transaction id.
    pending: HashMap<u16, PendingEntry>,
    /// If `in_flight >= N`, new requests go here until capacity opens up.
    queued: VecDeque<TaskCommand>,
    /// Monotonically-increasing transaction counter (skips 0).
    next_txn_id: u16,
    /// Mirror of `pending.len()` broadcast to `AsyncClientCore`.
    in_flight: usize,
    /// Sender half of the pending-count watch.
    pending_count_tx: watch::Sender<usize>,
    /// Optional traffic-event notifier (guarded by `traffic` feature).
    #[cfg(feature = "traffic")]
    notifier: NotifierStore,
}

impl<T: AsyncTransport + Send + 'static, const N: usize> ClientTask<T, N> {
    // ── Constructor ──────────────────────────────────────────────────────────

    /// Creates a new task.  Call [`run`](Self::run) to start the event loop.
    pub(crate) fn new(
        connect_fn: ConnectFactory<T>,
        cmd_rx: mpsc::Receiver<TaskCommand>,
        pending_count_tx: watch::Sender<usize>,
        #[cfg(feature = "traffic")] notifier: NotifierStore,
    ) -> Self {
        Self {
            transport: None,
            connect_fn,
            cmd_rx,
            pending: HashMap::new(),
            queued: VecDeque::new(),
            next_txn_id: 1,
            in_flight: 0,
            pending_count_tx,
            #[cfg(feature = "traffic")]
            notifier,
        }
    }

    // ── Private helpers ──────────────────────────────────────────────────────

    /// Returns the next transaction id, wrapping around `u16::MAX` and never
    /// returning 0 (reserved as the "no txn_id" sentinel for serial frames).
    fn advance_txn_id(&mut self) -> u16 {
        let id = self.next_txn_id;
        self.next_txn_id = match self.next_txn_id.wrapping_add(1) {
            0 => 1,
            n => n,
        };
        id
    }

    /// Pushes the current pending count to the watch channel.
    fn update_pending_count(&self) {
        let _ = self.pending_count_tx.send(self.pending.len());
    }

    /// Calls the stored factory to open a fresh transport and stores it.
    async fn do_connect(&mut self) -> Result<(), MbusError> {
        self.transport = None;
        let transport = (self.connect_fn)().await?;
        self.transport = Some(transport);
        Ok(())
    }

    /// Encodes and sends a `TaskCommand::Request`, inserting the entry into the
    /// pending map on success.
    ///
    /// `Connect` variants arriving here are rejected with `MbusError::Unexpected`.
    async fn dispatch_request(&mut self, cmd: TaskCommand) {
        let (params, resp_tx) = match cmd {
            TaskCommand::Request { params, resp_tx } => (params, resp_tx),
            TaskCommand::Connect { resp_tx } => {
                let _ = resp_tx.send(Err(MbusError::Unexpected));
                return;
            }
            // Disconnect is handled in handle_command before reaching here.
            TaskCommand::Disconnect => return,
        };

        // Resolve txn_id and ttype before taking a mutable transport borrow.
        let ttype = match &self.transport {
            Some(t) => t.transport_type(),
            None => {
                let _ = resp_tx.send(Err(MbusError::ConnectionClosed));
                return;
            }
        };
        let txn_id = self.advance_txn_id();
        let frame = match encode_request(txn_id, &params, ttype) {
            Ok(f) => f,
            Err(e) => {
                let _ = resp_tx.send(Err(e));
                return;
            }
        };

        // Capture unit before moving `params` into the pending map.
        #[cfg(feature = "traffic")]
        let unit = params.unit();

        // Take a mutable transport borrow only for the duration of the send.
        // Using a scoped expression so NLL ends the borrow before we insert into
        // `self.pending` (which also needs `&mut self`).
        let send_result = match self.transport.as_mut() {
            Some(t) => t.send(&frame).await,
            None => Err(MbusError::ConnectionClosed),
        };

        match send_result {
            Ok(()) => {
                #[cfg(feature = "traffic")]
                self.fire_tx_frame(txn_id, unit, &frame);
                self.pending.insert(
                    txn_id,
                    PendingEntry {
                        resp_tx,
                        request: params,
                    },
                );
                self.in_flight += 1;
                self.update_pending_count();
            }
            Err(e) => {
                #[cfg(feature = "traffic")]
                self.fire_tx_error(txn_id, unit, e);
                let _ = resp_tx.send(Err(e));
            }
        }
    }

    /// Matches a received frame to its pending entry and resolves the request.
    fn process_frame(&mut self, frame: &HVec<u8, MAX_ADU_FRAME_LEN>) {
        let ttype = match &self.transport {
            Some(t) => t.transport_type(),
            None => return,
        };

        let (decoded_txn_id, _unit, inner) = match decode_response(frame, ttype) {
            Ok(v) => v,
            Err(e) => {
                // Hard framing error — no txn_id recoverable; fail first pending entry.
                self.fail_entry(0, e);
                return;
            }
        };

        let key = self.resolve_key(decoded_txn_id);
        if let Some(k) = key
            && let Some(entry) = self.pending.remove(&k)
        {
            self.in_flight = self.in_flight.saturating_sub(1);
            self.update_pending_count();

            #[cfg(feature = "traffic")]
            self.fire_rx_frame(decoded_txn_id, entry.request.unit(), frame);

            let result = inner.map(|response| fix_up_response(response, &entry.request));
            let _ = entry.resp_tx.send(result);
        }
        // Unsolicited frame → discard silently.
    }

    /// Determines the pending-map key for a decoded txn_id.
    ///
    /// - TCP (txn_id != 0): direct lookup in the map.
    /// - Serial (txn_id == 0): take the first (and only for N=1) pending entry.
    fn resolve_key(&self, decoded_txn_id: u16) -> Option<u16> {
        if decoded_txn_id != 0 {
            self.pending
                .contains_key(&decoded_txn_id)
                .then_some(decoded_txn_id)
        } else {
            // Serial fallback — any pending entry is the one (PIPELINE=1 enforces this).
            self.pending.keys().next().copied()
        }
    }

    /// Fails the pending entry best matching `raw_txn_id` with `error`.
    fn fail_entry(&mut self, raw_txn_id: u16, error: MbusError) {
        if let Some(k) = self.resolve_key(raw_txn_id)
            && let Some(entry) = self.pending.remove(&k)
        {
            self.in_flight = self.in_flight.saturating_sub(1);
            self.update_pending_count();
            let _ = entry.resp_tx.send(Err(error));
        }
    }

    /// Sends `ConnectionClosed` to every in-flight and queued request, then
    /// resets counters.  Does **not** clear the transport; callers that also
    /// want to close the transport must set `self.transport = None` afterwards.
    pub(crate) fn drain_all(&mut self) {
        for (_, entry) in self.pending.drain() {
            let _ = entry.resp_tx.send(Err(MbusError::ConnectionClosed));
        }
        for cmd in self.queued.drain(..) {
            if let TaskCommand::Request { resp_tx, .. } = cmd {
                let _ = resp_tx.send(Err(MbusError::ConnectionClosed));
            }
            // TaskCommand::Connect / Disconnect have no resp_tx to drain.
        }
        self.in_flight = 0;
        self.update_pending_count();
    }

    /// Dispatches or queues a single request command; handles `Connect` and
    /// `Disconnect` inline.
    async fn handle_command(&mut self, cmd: TaskCommand) {
        match cmd {
            TaskCommand::Connect { resp_tx } => {
                let result = self.do_connect().await;
                let _ = resp_tx.send(result);
            }
            TaskCommand::Disconnect => {
                // Drain everything and close the transport so the pipeline is
                // clean.  A subsequent Connect command will reopen it.
                self.drain_all();
                self.transport = None;
            }
            req_cmd => {
                if self.in_flight < N {
                    self.dispatch_request(req_cmd).await;
                } else {
                    self.queued.push_back(req_cmd);
                }
            }
        }
    }

    // ── Traffic hooks (no-ops when feature is off) ────────────────────────────

    #[cfg(feature = "traffic")]
    fn fire_tx_frame(
        &self,
        txn_id: u16,
        unit: mbus_core::transport::UnitIdOrSlaveAddr,
        frame: &[u8],
    ) {
        // Non-blocking try_lock: skip notification if the lock is contested.
        if let Ok(mut g) = self.notifier.try_lock()
            && let Some(n) = g.as_mut()
        {
            n.on_tx_frame(txn_id, unit, frame);
        }
    }

    #[cfg(feature = "traffic")]
    fn fire_tx_error(
        &self,
        txn_id: u16,
        unit: mbus_core::transport::UnitIdOrSlaveAddr,
        err: MbusError,
    ) {
        if let Ok(mut g) = self.notifier.try_lock()
            && let Some(n) = g.as_mut()
        {
            n.on_tx_error(txn_id, unit, err, &[]);
        }
    }

    #[cfg(feature = "traffic")]
    fn fire_rx_frame(
        &self,
        txn_id: u16,
        unit: mbus_core::transport::UnitIdOrSlaveAddr,
        frame: &[u8],
    ) {
        if let Ok(mut g) = self.notifier.try_lock()
            && let Some(n) = g.as_mut()
        {
            n.on_rx_frame(txn_id, unit, frame);
        }
    }

    // ── Main loop ────────────────────────────────────────────────────────────

    /// Runs the task loop.  Returns when the command channel is closed (i.e. all
    /// `AsyncClientCore` handles have been dropped).
    pub(crate) async fn run(mut self) {
        loop {
            // Fill pipeline from backlog before blocking on select!.
            while self.in_flight < N {
                match self.queued.pop_front() {
                    Some(cmd) => self.dispatch_request(cmd).await,
                    None => break,
                }
            }

            tokio::select! {
                // Receive a response frame from the transport.
                // `recv_if_active` suspends forever when transport is None or
                // there are no in-flight requests.
                recv_result = recv_if_active(&mut self.transport, self.in_flight) => {
                    match recv_result {
                        Ok(frame) => self.process_frame(&frame),
                        Err(_) => {
                            // Transport error — mark as disconnected, drain pending.
                            // The user can issue a new Connect command to reconnect.
                            self.transport = None;
                            self.drain_all();
                        }
                    }
                }

                // Receive a command from the public API.
                maybe_cmd = self.cmd_rx.recv() => {
                    match maybe_cmd {
                        // Channel closed — all API handles dropped.
                        None => return,
                        Some(cmd) => self.handle_command(cmd).await,
                    }
                }
            }
        }
    }
}

// ─── Module-level helpers ─────────────────────────────────────────────────────

/// Awaits one complete frame from the transport.
///
/// Suspends forever (via `std::future::pending()`) when:
/// - `transport` is `None` (not connected), or
/// - `in_flight == 0` (no outstanding requests — we never expect a response).
async fn recv_if_active<T: AsyncTransport>(
    transport: &mut Option<T>,
    in_flight: usize,
) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    match transport.as_mut() {
        Some(t) if in_flight > 0 => t.recv().await,
        _ => std::future::pending().await,
    }
}

// ─── Response fix-up ──────────────────────────────────────────────────────────

/// Overwrites the placeholder `address` (and for bit/register reads, the
/// quantity) on responses where [`decode_response`] cannot know the original
/// request parameters.
///
/// Server *read* responses do not echo the requested starting address back;
/// `decode_response` initialises it to 0 as a placeholder.  This function
/// replaces that placeholder with the real values from `original`.
///
/// Server *write* responses echo address/quantity directly, so they need no
/// fix-up and fall through to the catch-all arm.
fn fix_up_response(
    response: crate::client::response::ClientResponse,
    original: &crate::client::command::ClientRequest,
) -> crate::client::response::ClientResponse {
    use crate::client::command::ClientRequest as Q;
    use crate::client::response::ClientResponse as R;

    match (response, original) {
        // ── FC01: Read Multiple Coils ─────────────────────────────────────
        #[cfg(feature = "coils")]
        (
            R::Coils(raw),
            Q::ReadMultipleCoils {
                address, quantity, ..
            },
        ) => {
            use mbus_core::models::coil::Coils;
            Coils::new(*address, *quantity)
                .and_then(|c| c.with_values(raw.values(), *quantity))
                .map(R::Coils)
                .unwrap_or_else(|_| R::Coils(raw))
        }

        // ── FC02: Read Discrete Inputs ────────────────────────────────────
        #[cfg(feature = "discrete-inputs")]
        (
            R::DiscreteInputs(raw),
            Q::ReadDiscreteInputs {
                address, quantity, ..
            },
        ) => {
            use mbus_core::models::discrete_input::DiscreteInputs;
            DiscreteInputs::new(*address, *quantity)
                .and_then(|d| d.with_values(raw.values(), *quantity))
                .map(R::DiscreteInputs)
                .unwrap_or_else(|_| R::DiscreteInputs(raw))
        }

        // ── FC03: Read Holding Registers ──────────────────────────────────
        #[cfg(feature = "registers")]
        (
            R::Registers(raw),
            Q::ReadHoldingRegisters {
                address, quantity, ..
            },
        ) => {
            use mbus_core::models::register::Registers;
            Registers::new(*address, *quantity)
                .and_then(|r| r.with_values(&raw.values()[..*quantity as usize], *quantity))
                .map(R::Registers)
                .unwrap_or_else(|_| R::Registers(raw))
        }

        // ── FC04: Read Input Registers ────────────────────────────────────
        #[cfg(feature = "registers")]
        (
            R::Registers(raw),
            Q::ReadInputRegisters {
                address, quantity, ..
            },
        ) => {
            use mbus_core::models::register::Registers;
            Registers::new(*address, *quantity)
                .and_then(|r| r.with_values(&raw.values()[..*quantity as usize], *quantity))
                .map(R::Registers)
                .unwrap_or_else(|_| R::Registers(raw))
        }

        // ── FC17: Read/Write Multiple Registers ───────────────────────────
        #[cfg(feature = "registers")]
        (
            R::Registers(raw),
            Q::ReadWriteMultipleRegisters {
                read_address,
                read_quantity,
                ..
            },
        ) => {
            use mbus_core::models::register::Registers;
            Registers::new(*read_address, *read_quantity)
                .and_then(|r| {
                    r.with_values(&raw.values()[..*read_quantity as usize], *read_quantity)
                })
                .map(R::Registers)
                .unwrap_or_else(|_| R::Registers(raw))
        }

        // ── FC18: Read FIFO Queue ─────────────────────────────────────────
        #[cfg(feature = "fifo")]
        (R::FifoQueue(raw), Q::ReadFifoQueue { address, .. }) => {
            use mbus_core::models::fifo_queue::{FifoQueue, MAX_FIFO_QUEUE_COUNT_PER_PDU};
            let length = raw.length();
            let mut arr = [0u16; MAX_FIFO_QUEUE_COUNT_PER_PDU];
            arr[..length].copy_from_slice(raw.queue());
            R::FifoQueue(FifoQueue::new(*address).with_values(arr, length))
        }

        // All write responses and diagnostics already carry correct
        // addresses/values echoed from the server — no fix-up needed.
        (r, _) => r,
    }
}
