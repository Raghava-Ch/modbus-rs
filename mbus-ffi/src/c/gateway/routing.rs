//! C-visible routing table.
//!
//! Combines exact unit-ID and contiguous unit-ID-range entries in a single
//! fixed-capacity policy. Both kinds are populated from the C side via
//! [`mbus_gateway_add_unit_route`](super::gateway::mbus_gateway_add_unit_route)
//! and [`mbus_gateway_add_range_route`](super::gateway::mbus_gateway_add_range_route).

use heapless::Vec as HVec;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::GatewayRoutingPolicy;

/// Maximum number of exact-unit routing entries per gateway instance.
pub const MAX_UNIT_ROUTES: usize = 32;
/// Maximum number of unit-ID-range routing entries per gateway instance.
pub const MAX_RANGE_ROUTES: usize = 8;

#[derive(Clone, Copy)]
struct UnitEntry {
    unit: u8,
    channel: usize,
}

#[derive(Clone, Copy)]
struct RangeEntry {
    min: u8,
    max: u8,
    channel: usize,
}

/// Routing policy backed by C-populated heapless arrays.
pub struct CGatewayRouter {
    units: HVec<UnitEntry, MAX_UNIT_ROUTES>,
    ranges: HVec<RangeEntry, MAX_RANGE_ROUTES>,
}

impl CGatewayRouter {
    pub const fn new() -> Self {
        Self {
            units: HVec::new(),
            ranges: HVec::new(),
        }
    }

    /// Register an exact-unit-ID route. Returns `false` if the table is full
    /// or the unit is already registered.
    pub fn add_unit(&mut self, unit: u8, channel: usize) -> bool {
        if self.units.iter().any(|e| e.unit == unit) {
            return false;
        }
        self.units.push(UnitEntry { unit, channel }).is_ok()
    }

    /// Register a contiguous range route. Returns `false` if the table is
    /// full or `min > max`.
    pub fn add_range(&mut self, min: u8, max: u8, channel: usize) -> bool {
        if min > max {
            return false;
        }
        self.ranges
            .push(RangeEntry { min, max, channel })
            .is_ok()
    }
}

impl Default for CGatewayRouter {
    fn default() -> Self {
        Self::new()
    }
}

impl GatewayRoutingPolicy for CGatewayRouter {
    fn route(&self, unit: UnitIdOrSlaveAddr) -> Option<usize> {
        let id = unit.get();
        if let Some(entry) = self.units.iter().find(|e| e.unit == id) {
            return Some(entry.channel);
        }
        self.ranges
            .iter()
            .find(|r| id >= r.min && id <= r.max)
            .map(|r| r.channel)
    }
}
