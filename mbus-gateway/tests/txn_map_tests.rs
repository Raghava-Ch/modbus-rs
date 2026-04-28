//! Tests for the TxnMap transaction-ID remapping table.

use mbus_gateway::{SerialTxnMap, TxnMap};

// ─────────────────────────────────────────────────────────────────────────────
// Basic operations
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn allocate_and_lookup() {
    let mut map: TxnMap<4> = TxnMap::new();

    let internal = map.allocate(0x0001, 0).expect("should allocate");
    let entry = map.lookup(internal).expect("should find entry");

    assert_eq!(entry.upstream_txn, 0x0001);
    assert_eq!(entry.session_id, 0);
    assert_eq!(entry.internal_txn, internal);
}

#[test]
fn allocate_multiple_and_lookup() {
    let mut map: TxnMap<4> = TxnMap::new();

    let a = map.allocate(0xAAAA, 0).unwrap();
    let b = map.allocate(0xBBBB, 1).unwrap();
    let c = map.allocate(0xCCCC, 0).unwrap();

    assert_eq!(map.lookup(a).unwrap().upstream_txn, 0xAAAA);
    assert_eq!(map.lookup(b).unwrap().upstream_txn, 0xBBBB);
    assert_eq!(map.lookup(c).unwrap().upstream_txn, 0xCCCC);

    assert_eq!(map.lookup(a).unwrap().session_id, 0);
    assert_eq!(map.lookup(b).unwrap().session_id, 1);
}

#[test]
fn lookup_missing_returns_none() {
    let map: TxnMap<4> = TxnMap::new();
    assert!(map.lookup(0xDEAD).is_none());
}

// ─────────────────────────────────────────────────────────────────────────────
// Remove
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn remove_returns_entry_and_shrinks_map() {
    let mut map: TxnMap<4> = TxnMap::new();
    let internal = map.allocate(0x1234, 2).unwrap();

    assert_eq!(map.len(), 1);

    let entry = map.remove(internal).expect("should remove");
    assert_eq!(entry.upstream_txn, 0x1234);
    assert_eq!(entry.session_id, 2);
    assert_eq!(map.len(), 0);
    assert!(map.is_empty());
}

#[test]
fn remove_missing_returns_none() {
    let mut map: TxnMap<4> = TxnMap::new();
    assert!(map.remove(0x9999).is_none());
}

#[test]
fn remove_only_removes_target() {
    let mut map: TxnMap<4> = TxnMap::new();
    let a = map.allocate(0xAA, 0).unwrap();
    let b = map.allocate(0xBB, 0).unwrap();

    map.remove(a).unwrap();

    assert!(map.lookup(a).is_none());
    assert!(map.lookup(b).is_some());
    assert_eq!(map.len(), 1);
}

// ─────────────────────────────────────────────────────────────────────────────
// Capacity limits
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn allocate_returns_none_when_full() {
    let mut map: TxnMap<2> = TxnMap::new();
    let a = map.allocate(0x0001, 0);
    let b = map.allocate(0x0002, 0);
    let c = map.allocate(0x0003, 0); // map is full

    assert!(a.is_some());
    assert!(b.is_some());
    assert!(c.is_none(), "should return None when map is full");
}

#[test]
fn allocate_after_remove_succeeds() {
    let mut map: TxnMap<1> = TxnMap::new();

    let first = map.allocate(0x0001, 0).unwrap();
    assert!(map.allocate(0x0002, 0).is_none()); // full

    map.remove(first).unwrap();

    // After freeing a slot we can allocate again.
    assert!(map.allocate(0x0002, 0).is_some());
}

// ─────────────────────────────────────────────────────────────────────────────
// Counter wrapping
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn internal_txn_ids_are_unique_and_incrementing() {
    let mut map: TxnMap<4> = TxnMap::new();

    let a = map.allocate(1, 0).unwrap();
    let b = map.allocate(2, 0).unwrap();
    let c = map.allocate(3, 0).unwrap();

    assert_ne!(a, b);
    assert_ne!(b, c);
    assert_ne!(a, c);
}

// ─────────────────────────────────────────────────────────────────────────────
// SerialTxnMap (capacity 1)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn serial_txn_map_capacity_one() {
    let mut map: SerialTxnMap = SerialTxnMap::new();

    let id = map.allocate(0xBEEF, 0).unwrap();
    assert!(map.allocate(0xDEAD, 0).is_none());

    map.remove(id).unwrap();
    assert!(map.allocate(0xDEAD, 0).is_some());
}

// ─────────────────────────────────────────────────────────────────────────────
// Default constructor
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn default_creates_empty_map() {
    let map: TxnMap<8> = TxnMap::default();
    assert!(map.is_empty());
    assert_eq!(map.len(), 0);
}
