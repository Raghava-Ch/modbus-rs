//! Tests for routing policy implementations.

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{
    GatewayRoutingPolicy, PassthroughRouter, RangeRouteTable, UnitIdRewriteRouter, UnitRouteTable,
};
use mbus_core::errors::MbusError;

fn uid(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new(v).expect("valid unit id")
}

// ─────────────────────────────────────────────────────────────────────────────
// UnitRouteTable
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_route_table_basic_routing() {
    let mut table: UnitRouteTable<4> = UnitRouteTable::new();
    table.add(uid(1), 0).unwrap();
    table.add(uid(2), 1).unwrap();
    table.add(uid(3), 0).unwrap();

    assert_eq!(table.route(uid(1)), Some(0));
    assert_eq!(table.route(uid(2)), Some(1));
    assert_eq!(table.route(uid(3)), Some(0));
}

#[test]
fn unit_route_table_miss() {
    let mut table: UnitRouteTable<4> = UnitRouteTable::new();
    table.add(uid(1), 0).unwrap();

    assert_eq!(table.route(uid(99)), None);
}

#[test]
fn unit_route_table_empty() {
    let table: UnitRouteTable<4> = UnitRouteTable::new();
    assert!(table.is_empty());
    assert_eq!(table.route(uid(1)), None);
}

#[test]
fn unit_route_table_rejects_duplicate() {
    let mut table: UnitRouteTable<4> = UnitRouteTable::new();
    table.add(uid(1), 0).unwrap();
    let err = table.add(uid(1), 1).unwrap_err();
    assert_eq!(err, MbusError::InvalidAddress);
}

#[test]
fn unit_route_table_rejects_when_full() {
    let mut table: UnitRouteTable<2> = UnitRouteTable::new();
    table.add(uid(1), 0).unwrap();
    table.add(uid(2), 0).unwrap();
    let err = table.add(uid(3), 0).unwrap_err();
    assert_eq!(err, MbusError::TooManyRequests);
}

#[test]
fn unit_route_table_len() {
    let mut table: UnitRouteTable<4> = UnitRouteTable::new();
    assert_eq!(table.len(), 0);
    table.add(uid(1), 0).unwrap();
    assert_eq!(table.len(), 1);
    table.add(uid(2), 1).unwrap();
    assert_eq!(table.len(), 2);
}

// ─────────────────────────────────────────────────────────────────────────────
// RangeRouteTable
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn range_route_table_basic_routing() {
    let mut table: RangeRouteTable<4> = RangeRouteTable::new();
    table.add(1, 32, 0).unwrap();
    table.add(33, 64, 1).unwrap();

    assert_eq!(table.route(uid(1)), Some(0));
    assert_eq!(table.route(uid(32)), Some(0));
    assert_eq!(table.route(uid(33)), Some(1));
    assert_eq!(table.route(uid(64)), Some(1));
}

#[test]
fn range_route_table_miss() {
    let mut table: RangeRouteTable<4> = RangeRouteTable::new();
    table.add(1, 32, 0).unwrap();

    assert_eq!(table.route(uid(100)), None);
}

#[test]
fn range_route_table_rejects_invalid_range() {
    let mut table: RangeRouteTable<4> = RangeRouteTable::new();
    let err = table.add(32, 1, 0).unwrap_err(); // min > max
    assert_eq!(err, MbusError::InvalidAddress);
}

#[test]
fn range_route_table_rejects_overlap() {
    let mut table: RangeRouteTable<4> = RangeRouteTable::new();
    table.add(1, 32, 0).unwrap();
    let err = table.add(20, 40, 1).unwrap_err(); // overlaps 1–32
    assert_eq!(err, MbusError::InvalidAddress);
}

#[test]
fn range_route_table_rejects_when_full() {
    let mut table: RangeRouteTable<2> = RangeRouteTable::new();
    table.add(1, 10, 0).unwrap();
    table.add(11, 20, 0).unwrap();
    let err = table.add(21, 30, 0).unwrap_err();
    assert_eq!(err, MbusError::TooManyRequests);
}

#[test]
fn range_route_table_single_unit_range() {
    let mut table: RangeRouteTable<4> = RangeRouteTable::new();
    table.add(5, 5, 0).unwrap(); // single unit

    assert_eq!(table.route(uid(5)), Some(0));
    assert_eq!(table.route(uid(4)), None);
    assert_eq!(table.route(uid(6)), None);
}

// ─────────────────────────────────────────────────────────────────────────────
// PassthroughRouter
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn passthrough_router_always_channel_zero() {
    let router = PassthroughRouter;
    for v in [1u8, 10, 100, 247] {
        assert_eq!(router.route(uid(v)), Some(0));
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// UnitIdRewriteRouter
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn unit_id_rewrite_router_positive_offset() {
    let mut inner: UnitRouteTable<4> = UnitRouteTable::new();
    inner.add(uid(1), 0).unwrap();
    let router = UnitIdRewriteRouter::new(inner, 100);

    // Routing still uses the original unit ID.
    assert_eq!(router.route(uid(1)), Some(0));

    // Rewrite shifts unit 1 → 101.
    assert_eq!(router.rewrite(uid(1)).get(), 101);
}

#[test]
fn unit_id_rewrite_router_negative_offset() {
    let mut inner: UnitRouteTable<4> = UnitRouteTable::new();
    inner.add(uid(50), 0).unwrap();
    let router = UnitIdRewriteRouter::new(inner, -10);

    assert_eq!(router.route(uid(50)), Some(0));
    assert_eq!(router.rewrite(uid(50)).get(), 40);
}

#[test]
fn unit_id_rewrite_router_clamps_to_valid_range() {
    let inner = PassthroughRouter;
    let router = UnitIdRewriteRouter::new(inner, 200);

    // 100 + 200 = 300 → clamped to 247.
    assert_eq!(router.rewrite(uid(100)).get(), 247);
}

#[test]
fn unit_id_rewrite_router_clamps_minimum_to_one() {
    let inner = PassthroughRouter;
    let router = UnitIdRewriteRouter::new(inner, -200);

    // 5 - 200 = -195 → clamped to 1.
    assert_eq!(router.rewrite(uid(5)).get(), 1);
}
