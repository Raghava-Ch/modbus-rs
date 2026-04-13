//! # Service Resilience Configuration
//!
//! Configurable timeout, priority-queueing, and response-retry infrastructure
//! for [`ServerServices`](super::ServerServices).
//!
//! ## Overview
//!
//! - [`TimeoutConfig`]: millisecond thresholds for app callbacks, individual
//!   sends, response retry cadence, and per-request end-to-end deadlines.
//! - [`RequestPriority`]: enum that classifies each Modbus request so that
//!   high-importance requests are dispatched first when the queue has depth.
//! - [`ResilienceConfig`]: the single struct passed to
//!   [`ServerServices::new`](super::ServerServices::new) to activate all
//!   resilience features.
//! - [`RequestQueue`] / [`ResponseQueue`]: fixed-capacity heapless collections
//!   used by `ServerServices` at runtime.

use heapless::{Deque, Vec};
use mbus_core::{
    data_unit::common::MAX_ADU_FRAME_LEN,
    function_codes::public::FunctionCode,
    transport::UnitIdOrSlaveAddr,
};

// ---------------------------------------------------------------------------
// Clock abstraction
// ---------------------------------------------------------------------------

/// Application-supplied callback that returns the current elapsed time in
/// milliseconds (monotonically increasing).
///
/// No cryptographic properties are required; a simple system tick counter is
/// sufficient.  The value wraps after ~584 million years at nanosecond
/// resolution and much later at millisecond resolution, so overflow is safe
/// to ignore for practical deployments.
///
/// If a clock is not available on the target platform, keep
/// [`ResilienceConfig::clock_fn`] as `None` — all time-based features are
/// automatically disabled.
pub type ClockFn = fn() -> u64;

// ---------------------------------------------------------------------------
// OverflowPolicy
// ---------------------------------------------------------------------------

/// Policy for handling the case where the response retry queue becomes nearly full.
///
/// This is important for asymmetric state protection: when an application state change
/// succeeds but the response cannot be sent, the client must not time out believing
/// the operation failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Drop the response if the retry queue is full (legacy behavior).
    ///
    /// This risks asymmetric state: the operation succeeded on the server but the
    /// client never receives confirmation. Use this only if you can tolerate
    /// potential client retries and duplicate operations on the server side.
    DropResponse,
    /// Reject incoming requests when the response queue reaches 80% utilization.
    ///
    /// Prevents state changes from being made when there's insufficient space to
    /// guarantee response delivery. Addressed unicast requests receive a
    /// `TooManyRequests`-mapped exception response when this policy is active and
    /// the queue is under pressure. With the current exception mapping this is
    /// emitted as `ServerDeviceFailure`. Broadcast and misaddressed frames remain
    /// silently discarded because address filtering runs before back-pressure.
    RejectRequest,
}

// ---------------------------------------------------------------------------
// TimeoutConfig
// ---------------------------------------------------------------------------

/// Timeout thresholds controlling how long each phase of request processing
/// may take before an action is taken.
///
/// All fields default to `0`, which **disables** that specific check.  A
/// [`ClockFn`] must also be provided via [`ResilienceConfig::clock_fn`] for
/// any threshold to have effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeoutConfig {
    /// Maximum milliseconds an application callback may take.
    ///
    /// This is detected *post-hoc*: the callback always runs to completion.
    /// When exceeded, a `debug`-level log entry is emitted.  `0 = disabled`.
    pub app_callback_ms: u32,

    /// Maximum milliseconds allowed for a single `Transport::send()` call.
    ///
    /// If the send takes longer than this threshold a warning is logged.
    /// A failed send is always queued for retry regardless of this setting.
    /// `0 = disabled`.
    pub send_ms: u32,

    /// Minimum delay in milliseconds between retry attempts for a queued
    /// response after a failed `Transport::send()`.
    ///
    /// This delay is enforced by [`ServerServices::poll`](super::ServerServices::poll)
    /// when draining the response queue. A configured [`ClockFn`] is required;
    /// when no clock is provided this delay is ignored.
    ///
    /// `0 = disabled` (retry cadence is driven by poll frequency).
    pub response_retry_interval_ms: u32,

    /// Maximum time in milliseconds a queued request may wait before it is
    /// considered stale and discarded.
    ///
    /// - When [`TimeoutConfig::strict_mode`] is `false` (default): the stale
    ///   request is silently dropped.
    /// - When `strict_mode` is `true`: a Modbus exception response
    ///   (`GatewayPathUnavailable`) is sent to the requester before dropping.
    ///
    /// `0 = disabled` — requests wait in the queue indefinitely.
    pub request_deadline_ms: u32,

    /// Controls the action taken when a queued request exceeds
    /// [`request_deadline_ms`](Self::request_deadline_ms).
    ///
    /// - `false` (default): silently drop the stale request.
    /// - `true`: send a `GatewayPathUnavailable` exception response before
    ///   dropping.
    pub strict_mode: bool,

    /// Policy for handling response queue overflow.
    ///
    /// Determines whether to drop responses (legacy) or reject incoming addressed
    /// unicast requests when the response retry queue is under pressure. See
    /// [`OverflowPolicy`] for details. Defaults to `DropResponse` for backward
    /// compatibility.
    pub overflow_policy: OverflowPolicy,
}

impl Default for TimeoutConfig {
    fn default() -> Self {
        Self {
            app_callback_ms: 0,
            send_ms: 0,
            response_retry_interval_ms: 0,
            request_deadline_ms: 0,
            strict_mode: false,
            overflow_policy: OverflowPolicy::DropResponse,
        }
    }
}

// ---------------------------------------------------------------------------
// RequestPriority
// ---------------------------------------------------------------------------

/// Priority level assigned to a Modbus request based on its function code.
///
/// When multiple requests are buffered in the [`RequestQueue`], higher-priority
/// items are dispatched first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum RequestPriority {
    /// All function codes not otherwise classified.
    Other = 0,
    /// Read operations (FC01, FC02, FC03, FC04).
    Read = 1,
    /// Write operations (FC05, FC06, FC0F, FC10, FC16).
    Write = 2,
    /// Maintenance and diagnostic operations (FC08, FC11, FC12, FC2B).
    Maintenance = 3,
}

impl RequestPriority {
    /// Derives the priority level from a Modbus function code.
    pub fn from_function_code(fc: FunctionCode) -> Self {
        use FunctionCode::*;
        match fc {
            ReadCoils | ReadDiscreteInputs | ReadHoldingRegisters | ReadInputRegisters
            | ReadFifoQueue | ReadFileRecord => Self::Read,

            WriteSingleCoil
            | WriteSingleRegister
            | WriteMultipleCoils
            | WriteMultipleRegisters
            | MaskWriteRegister
            | ReadWriteMultipleRegisters
            | WriteFileRecord => Self::Write,

            Diagnostics
            | GetCommEventCounter
            | GetCommEventLog
            | ReportServerId
            | EncapsulatedInterfaceTransport => Self::Maintenance,

            _ => Self::Other,
        }
    }
}

// ---------------------------------------------------------------------------
// ResilienceConfig
// ---------------------------------------------------------------------------

/// Top-level resilience configuration for [`ServerServices`](super::ServerServices).
///
/// Pass an instance to [`ServerServices::new`](super::ServerServices::new) to
/// activate timeout tracking, priority request queueing, and response retry.
/// All settings default to **disabled** — callers that do not need resilience
/// can simply pass `ResilienceConfig::default()`.
///
/// ## Deterministic retry cadence
///
/// Set [`TimeoutConfig::response_retry_interval_ms`] to enforce a minimum delay
/// between queued response retry attempts. This makes retry behaviour
/// deterministic with respect to your application clock rather than poll loop
/// frequency.
///
/// A configured [`ClockFn`] is required for interval enforcement. When
/// [`ResilienceConfig::clock_fn`] is `None`, retries are still supported but
/// remain poll-driven.
///
/// # Example
///
/// ```rust,ignore
/// use mbus_server::{OverflowPolicy, ResilienceConfig, TimeoutConfig};
///
/// let resilience = ResilienceConfig {
///     timeouts: TimeoutConfig {
///         app_callback_ms: 50,
///         send_ms: 100,
///         response_retry_interval_ms: 25,
///         request_deadline_ms: 500,
///         strict_mode: false,
///         overflow_policy: OverflowPolicy::DropResponse,
///     },
///     clock_fn: Some(my_clock_ms),
///     max_send_retries: 3,
///     enable_priority_queue: true,
///     enable_broadcast_writes: false,
/// };
/// ```
#[derive(Debug, Clone, Copy)]
pub struct ResilienceConfig {
    /// Timeout thresholds for individual processing phases.
    pub timeouts: TimeoutConfig,

    /// Application-supplied monotonic millisecond clock.
    ///
    /// Required for any time-based feature.  When `None`, all threshold checks
    /// are skipped regardless of [`TimeoutConfig`] values.
    pub clock_fn: Option<ClockFn>,

    /// Maximum number of times a failed `transport.send()` should be retried
    /// from the response queue.
    ///
    /// `0` disables response retries.  Defaults to `3`.
    pub max_send_retries: u8,

    /// When `true`, incoming requests are pushed onto the [`RequestQueue`]
    /// rather than dispatched immediately.  This enables priority-ordered
    /// dispatch when multiple requests are buffered at the same time.
    ///
    /// When `false` (default), requests are dispatched immediately upon
    /// receipt for minimal latency.
    pub enable_priority_queue: bool,

    /// Enables Serial broadcast write handling without emitting any response.
    ///
    /// This is only honored for Serial transports. TCP and other point-to-point
    /// transports continue to silently drop broadcast traffic.
    pub enable_broadcast_writes: bool,
}

impl Default for ResilienceConfig {
    fn default() -> Self {
        Self {
            timeouts: TimeoutConfig::default(),
            clock_fn: None,
            max_send_retries: 3,
            enable_priority_queue: false,
            enable_broadcast_writes: false,
        }
    }
}

// ---------------------------------------------------------------------------
// PendingRequest
// ---------------------------------------------------------------------------

/// A received ADU frame waiting for priority-ordered dispatch.
pub(crate) struct PendingRequest {
    /// Raw ADU frame bytes.
    pub(crate) frame: Vec<u8, MAX_ADU_FRAME_LEN>,
    /// Priority derived from the function code.
    pub(crate) priority: RequestPriority,
    /// Clock value (ms) when this request was placed into the queue.
    /// `0` when no clock is available.
    pub(crate) received_at_ms: u64,
}

// ---------------------------------------------------------------------------
// RequestQueue
// ---------------------------------------------------------------------------

/// Bounded priority queue for incoming Modbus requests.
///
/// `N` is the maximum number of requests that can be pending simultaneously.
/// When the queue is full, new requests are dropped (with a debug log).
pub(crate) struct RequestQueue<const N: usize> {
    items: Vec<PendingRequest, N>,
}

impl<const N: usize> RequestQueue<N> {
    pub(crate) const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Enqueues a request.  Returns `true` on success, `false` if the queue is full.
    pub(crate) fn push(&mut self, req: PendingRequest) -> bool {
        self.items.push(req).is_ok()
    }

    /// Removes and returns the highest-priority pending request.
    ///
    /// Among items of equal priority the most-recently-added is returned
    /// (arbitrary LIFO within a priority tier — FIFO is not guaranteed).
    pub(crate) fn pop_highest_priority(&mut self) -> Option<PendingRequest> {
        if self.items.is_empty() {
            return None;
        }
        let mut max_idx = 0;
        for (i, item) in self.items.iter().enumerate() {
            if item.priority > self.items[max_idx].priority {
                max_idx = i;
            }
        }
        Some(self.items.swap_remove(max_idx))
    }

    /// Removes all requests whose age exceeds `deadline_ms`.
    ///
    /// Returns the number of expired requests.  Calls to this method with
    /// `deadline_ms == 0` are no-ops.
    pub(crate) fn expire_stale(&mut self, now_ms: u64, deadline_ms: u32) -> usize {
        if deadline_ms == 0 {
            return 0;
        }
        let limit = deadline_ms as u64;
        let before = self.items.len();
        let mut i = 0;
        while i < self.items.len() {
            if now_ms.saturating_sub(self.items[i].received_at_ms) > limit {
                self.items.swap_remove(i);
                // swap_remove moves the last item to position `i`; do not advance.
            } else {
                i += 1;
            }
        }
        before - self.items.len()
    }

    /// Removes and returns all requests whose age exceeds `deadline_ms`.
    ///
    /// Calls with `deadline_ms == 0` return an empty vector.
    pub(crate) fn take_expired(&mut self, now_ms: u64, deadline_ms: u32) -> Vec<PendingRequest, N> {
        let mut expired: Vec<PendingRequest, N> = Vec::new();
        if deadline_ms == 0 {
            return expired;
        }

        let limit = deadline_ms as u64;
        let mut i = 0;
        while i < self.items.len() {
            if now_ms.saturating_sub(self.items[i].received_at_ms) > limit {
                let item = self.items.swap_remove(i);
                let _ = expired.push(item);
                // swap_remove moves last item into `i`; re-check same index.
            } else {
                i += 1;
            }
        }
        expired
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
}

// ---------------------------------------------------------------------------
// PendingResponse
// ---------------------------------------------------------------------------

/// A serialised response frame waiting to be (re-)sent after a send failure.
pub(crate) struct PendingResponse {
    /// Serialised ADU bytes to transmit.
    pub(crate) frame: Vec<u8, MAX_ADU_FRAME_LEN>,
    /// Transaction identifier associated with this response.
    pub(crate) txn_id: u16,
    /// Address metadata (TCP unit id or serial slave address).
    pub(crate) unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    /// How many send attempts have already been made.
    pub(crate) retry_count: u8,
    /// Clock value (ms) when this response was first queued.
    pub(crate) queued_at_ms: u64,
}

// ---------------------------------------------------------------------------
// ResponseQueue
// ---------------------------------------------------------------------------

/// FIFO queue for responses that could not be sent and are awaiting retry.
///
/// `N` is the maximum number of pending responses.  When the queue is full,
/// new failures are dropped (the response is lost).
pub(crate) struct ResponseQueue<const N: usize> {
    items: Deque<PendingResponse, N>,
}

impl<const N: usize> ResponseQueue<N> {
    pub(crate) const fn new() -> Self {
        Self {
            items: Deque::new(),
        }
    }

    /// Enqueues a response for later retry.  Returns `true` on success,
    /// `false` if the queue is full.
    pub(crate) fn push_back(&mut self, resp: PendingResponse) -> bool {
        self.items.push_back(resp).is_ok()
    }

    /// Removes and returns the oldest queued response.
    pub(crate) fn pop_front(&mut self) -> Option<PendingResponse> {
        self.items.pop_front()
    }

    pub(crate) fn len(&self) -> usize {
        self.items.len()
    }
}
