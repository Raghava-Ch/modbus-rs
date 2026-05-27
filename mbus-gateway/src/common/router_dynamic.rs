//! Dynamic routing policy types for std/async environments.
//!
//! Provides heap-allocated, dynamically extendable routing tables backed by standard `Vec`.
//! They implement [`GatewayRoutingPolicy`] and allow adding routes dynamically at runtime
//! without compile-time generic array size constraints.

use mbus_core::{errors::MbusError, transport::UnitIdOrSlaveAddr};
use crate::common::router::{GatewayRoutingPolicy, UnitRangeRoute, UnitRouteEntry};

/// Exact unit-ID → channel mapping backed by a dynamically-allocated, extendable `std::vec::Vec`.
///
/// # Example
/// ```rust
/// use mbus_gateway::{GatewayRoutingPolicy, DynamicUnitRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
///
/// let mut table = DynamicUnitRouteTable::new();
/// table.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();
/// table.add(UnitIdOrSlaveAddr::new(2).unwrap(), 1).unwrap();
///
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(1).unwrap()), Some(0));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(3).unwrap()), None);
/// ```
#[derive(Debug, Clone)]
pub struct DynamicUnitRouteTable {
    entries: Vec<UnitRouteEntry>,
}

impl DynamicUnitRouteTable {
    /// Create an empty dynamic routing table.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Add a mapping from `unit` to `channel`.
    ///
    /// Returns `Err(MbusError::InvalidAddress)` if `unit` is already registered.
    pub fn add(&mut self, unit: UnitIdOrSlaveAddr, channel: usize) -> Result<(), MbusError> {
        if self.entries.iter().any(|e| e.unit == unit.get()) {
            return Err(MbusError::InvalidAddress);
        }
        self.entries.push(UnitRouteEntry {
            unit: unit.get(),
            channel,
        });
        Ok(())
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

impl Default for DynamicUnitRouteTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayRoutingPolicy for DynamicUnitRouteTable {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        self.entries
            .iter()
            .find(|e| e.unit == unit.get())
            .map(|e| e.channel)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DynamicRangeRouteTable
// ─────────────────────────────────────────────────────────────────────────────

/// Range-based routing table backed by a dynamically-allocated, extendable `std::vec::Vec`.
///
/// All unit IDs in `[min, max]` (inclusive) are forwarded to the associated
/// channel. Ranges must be non-overlapping; `add()` enforces this at runtime.
///
/// # Example
/// ```rust
/// use mbus_gateway::{GatewayRoutingPolicy, DynamicRangeRouteTable};
/// use mbus_core::transport::UnitIdOrSlaveAddr;
///
/// let mut table = DynamicRangeRouteTable::new();
/// table.add(1, 32, 0).unwrap();  // units 1–32 → channel 0
/// table.add(33, 64, 1).unwrap(); // units 33–64 → channel 1
///
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(10).unwrap()), Some(0));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(50).unwrap()), Some(1));
/// assert_eq!(table.route(UnitIdOrSlaveAddr::new(100).unwrap()), None);
/// ```
#[derive(Debug, Clone)]
pub struct DynamicRangeRouteTable {
    ranges: Vec<UnitRangeRoute>,
}

impl DynamicRangeRouteTable {
    /// Create an empty dynamic range routing table.
    pub fn new() -> Self {
        Self {
            ranges: Vec::new(),
        }
    }

    /// Add a range `[min, max]` → `channel` mapping.
    ///
    /// Returns `Err(MbusError::InvalidAddress)` if `min > max` or if the new
    /// range overlaps an existing entry.
    pub fn add(&mut self, min: u8, max: u8, channel: usize) -> Result<(), MbusError> {
        if min > max {
            return Err(MbusError::InvalidAddress);
        }
        // Check for overlap with existing ranges.
        if self.ranges.iter().any(|r| !(r.max < min || r.min > max)) {
            return Err(MbusError::InvalidAddress);
        }
        self.ranges.push(UnitRangeRoute { min, max, channel });
        Ok(())
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

impl Default for DynamicRangeRouteTable {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayRoutingPolicy for DynamicRangeRouteTable {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        let v = unit.get();
        self.ranges
            .iter()
            .find(|r| r.min <= v && v <= r.max)
            .map(|r| r.channel)
    }
}
