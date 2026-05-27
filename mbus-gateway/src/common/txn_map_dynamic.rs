//! Dynamic transaction-ID remapping table for std/async environments.
//!
//! Provides heap-allocated, dynamically extendable transaction mapping backed by standard `Vec`.
//! This avoids the need for compile-time generic in-flight capacities.

use crate::common::txn_map::TxnEntry;

/// Dynamically-allocated, extendable transaction-ID remapping table.
///
/// # Example
/// ```rust
/// use mbus_gateway::DynamicTxnMap;
///
/// let mut map = DynamicTxnMap::new();
/// let internal = map.allocate(0x0001, 0).unwrap();
/// let entry = map.lookup(internal).unwrap();
/// assert_eq!(entry.upstream_txn, 0x0001);
/// assert_eq!(entry.session_id, 0);
/// map.remove(internal).unwrap();
/// assert!(map.is_empty());
/// ```
#[derive(Debug, Clone)]
pub struct DynamicTxnMap {
    entries: Vec<TxnEntry>,
    /// Monotonically-incrementing internal transaction counter.
    next_internal: u16,
}

impl DynamicTxnMap {
    /// Create an empty dynamic map.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
            next_internal: 0,
        }
    }

    /// Allocate a new internal transaction ID for an upstream `(txn, session)` pair.
    pub fn allocate(&mut self, upstream_txn: u16, session_id: u8) -> Option<u16> {
        let internal = self.next_internal;
        self.next_internal = self.next_internal.wrapping_add(1);
        self.entries.push(TxnEntry {
            internal_txn: internal,
            upstream_txn,
            session_id,
        });
        Some(internal)
    }

    /// Look up an entry by its internal transaction ID.
    pub fn lookup(&self, internal_txn: u16) -> Option<&TxnEntry> {
        self.entries.iter().find(|e| e.internal_txn == internal_txn)
    }

    /// Remove and return the entry for `internal_txn`.
    ///
    /// Returns `None` if no such entry exists.
    pub fn remove(&mut self, internal_txn: u16) -> Option<TxnEntry> {
        let pos = self
            .entries
            .iter()
            .position(|e| e.internal_txn == internal_txn)?;
        Some(self.entries.swap_remove(pos))
    }

    /// Return the number of in-flight entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Return `true` when no entries are in flight.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Remove all in-flight entries belonging to `session_id`.
    /// Returns the count removed.
    pub fn remove_by_session(&mut self, session_id: u8) -> usize {
        let mut count = 0;
        let mut i = 0;
        while i < self.entries.len() {
            if self.entries[i].session_id == session_id {
                self.entries.swap_remove(i);
                count += 1;
            } else {
                i += 1;
            }
        }
        count
    }
}

impl Default for DynamicTxnMap {
    fn default() -> Self {
        Self::new()
    }
}
