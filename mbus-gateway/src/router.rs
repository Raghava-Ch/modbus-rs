//! Routing policy types.
//!
//! The gateway determines which downstream channel to use for an incoming
//! request by consulting a [`GatewayRoutingPolicy`].  Several built-in
//! implementations are provided; you can also supply your own.

use mbus_core::{errors::MbusError, transport::UnitIdOrSlaveAddr};

/// Core routing abstraction.
///
/// Given a Modbus unit ID (or serial slave address), return the index of the
/// downstream channel that should handle the request, or `None` if no route is
/// configured for that unit.
pub trait GatewayRoutingPolicy {
    /// Map a unit ID to a downstream channel index.
    ///
    /// Returns `Some(channel_idx)` when a route is found, `None` otherwise.
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize>;
}

// ─────────────────────────────────────────────────────────────────────────────
// UnitRouteTable
// ─────────────────────────────────────────────────────────────────────────────

struct UnitRouteEntry {
    unit: u8,
    channel: usize,
}

/// Exact unit-ID → channel mapping backed by a fixed-capacity heapless Vec.
///
/// # Const Generic
/// `N` — maximum number of routing entries.
///
/// # Example
/// ```rust
/// use mbus_gateway::{GatewayRoutingPolicy, UnitRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
///
/// let mut table: UnitRouteTable<4> = UnitRouteTable::new();
/// table.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
/// table.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap();
///
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(1).unwrap()), Some(0));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(3).unwrap()), None);
/// ```
pub struct UnitRouteTable<const N: usize> {
    entries: heapless::Vec<UnitRouteEntry, N>,
}

impl<const N: usize> UnitRouteTable<N> {
    /// Create an empty routing table.
    pub fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
        }
    }

    /// Add a mapping from `unit` to `channel`.
    ///
    /// Returns `Err(MbusError::InvalidAddress)` if `unit` is already registered.
    /// Returns `Err(MbusError::TooManyRequests)` if the table is full.
    pub fn add(&mut self, unit: UnitIdOrSlaveAddr, channel: usize) -> Result<(), MbusError> {
        if self.entries.iter().any(|e| e.unit == unit.get()) {
            return Err(MbusError::InvalidAddress);
        }
        self.entries
            .push(UnitRouteEntry {
                unit: unit.get(),
                channel,
            })
            .map_err(|_| MbusError::TooManyRequests)
    }

    /// Return the number of entries currently registered.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` if the table has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<const N: usize> Default for UnitRouteTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> GatewayRoutingPolicy for UnitRouteTable<N> {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        self.entries
            .iter()
            .find(|e| e.unit == unit.get())
            .map(|e| e.channel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RangeRouteTable
// ─────────────────────────────────────────────────────────────────────────────

/// A single unit-ID range mapped to one downstream channel.
#[derive(Debug, Clone, Copy)]
pub struct UnitRangeRoute {
    /// Inclusive minimum unit ID.
    pub min: u8,
    /// Inclusive maximum unit ID.
    pub max: u8,
    /// Downstream channel index.
    pub channel: usize,
}

/// Range-based routing table.
///
/// All unit IDs in `[min, max]` (inclusive) are forwarded to the associated
/// channel.  Ranges must be non-overlapping; `add()` enforces this at runtime.
///
/// # Const Generic
/// `N` — maximum number of range entries.
///
/// # Example
/// ```rust
/// use mbus_gateway::{GatewayRoutingPolicy, RangeRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
///
/// let mut table: RangeRouteTable<4> = RangeRouteTable::new();
/// table.add(1, 32, 0).unwrap();  // units 1–32 → channel 0
/// table.add(33, 64, 1).unwrap(); // units 33–64 → channel 1
///
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(10).unwrap()), Some(0));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(50).unwrap()), Some(1));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(100).unwrap()), None);
/// ```
pub struct RangeRouteTable<const N: usize> {
    ranges: heapless::Vec<UnitRangeRoute, N>,
}

impl<const N: usize> RangeRouteTable<N> {
    /// Create an empty range routing table.
    pub fn new() -> Self {
        Self {
            ranges: heapless::Vec::new(),
        }
    }

    /// Add a range `[min, max]` → `channel` mapping.
    ///
    /// Returns `Err(MbusError::InvalidAddress)` if `min > max` or if the new
    /// range overlaps an existing entry.
    /// Returns `Err(MbusError::TooManyRequests)` if the table is full.
    pub fn add(&mut self, min: u8, max: u8, channel: usize) -> Result<(), MbusError> {
        if min > max {
            return Err(MbusError::InvalidAddress);
        }
        // Check for overlap with existing ranges.
        if self
            .ranges
            .iter()
            .any(|r| !(r.max < min || r.min > max))
        {
            return Err(MbusError::InvalidAddress);
        }
        self.ranges
            .push(UnitRangeRoute { min, max, channel })
            .map_err(|_| MbusError::TooManyRequests)
    }

    /// Return the number of ranges currently registered.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Return `true` if the table has no ranges.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }
}

impl<const N: usize> Default for RangeRouteTable<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> GatewayRoutingPolicy for RangeRouteTable<N> {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        let v = unit.get();
        self.ranges
            .iter()
            .find(|r| r.min <= v && v <= r.max)
            .map(|r| r.channel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// PassthroughRouter
// ─────────────────────────────────────────────────────────────────────────────

/// No-configuration router that forwards every unit ID to channel 0.
///
/// Useful for single-downstream setups where all traffic goes to one bus.
///
/// # Example
/// ```rust
/// use mbus_gateway::{GatewayRoutingPolicy, PassthroughRouter};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
///
/// let router = PassthroughRouter;
/// assert_eq!(router.route(UnitIdOrSlaveAddr::new(42).unwrap()), Some(0));
/// ```
pub struct PassthroughRouter;

impl GatewayRoutingPolicy for PassthroughRouter {
    #[inline]
    fn route(&self, _unit: UnitIdOrSlaveAddr) -> Option<usize> {
        Some(0)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UnitIdRewriteRouter
// ─────────────────────────────────────────────────────────────────────────────

/// Wraps an inner routing policy and applies an additive offset to the unit ID
/// before constructing the downstream ADU.
///
/// The routing decision itself is delegated to the inner policy using the
/// *original* unit ID.  The rewritten unit ID is only used when building the
/// downstream frame (see [`rewrite`](UnitIdRewriteRouter::rewrite)).
///
/// Rewritten values are clamped to the valid unit-ID range `[1, 247]`.
pub struct UnitIdRewriteRouter<R: GatewayRoutingPolicy> {
    inner: R,
    /// Additive offset applied to the downstream unit ID.
    offset: i16,
}

impl<R: GatewayRoutingPolicy> UnitIdRewriteRouter<R> {
    /// Create a new rewriting router wrapping `inner` with the given `offset`.
    pub fn new(inner: R, offset: i16) -> Self {
        Self { inner, offset }
    }

    /// Compute the rewritten downstream unit ID for `unit`.
    ///
    /// The result is clamped to `[1, 247]`.
    pub fn rewrite(&self, unit: UnitIdOrSlaveAddr) -> UnitIdOrSlaveAddr {
        let raw = unit.get() as i16 + self.offset;
        let clamped = raw.max(1).min(247) as u8;
        UnitIdOrSlaveAddr::new(clamped).unwrap_or(unit)
    }

    /// Return a reference to the inner routing policy.
    pub fn inner(&self) -> &R {
        &self.inner
    }
}

impl<R: GatewayRoutingPolicy> GatewayRoutingPolicy for UnitIdRewriteRouter<R> {
    /// Route using the *original* unit ID (before offset rewrite).
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        self.inner.route(unit)
    }
}
