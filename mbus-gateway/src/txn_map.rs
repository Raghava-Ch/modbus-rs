//! Transaction-ID remapping table.
//!
//! When multiple upstream TCP clients connect simultaneously, each produces its
//! own set of transaction IDs.  The gateway re-numbers them to a monotonically
//! increasing internal counter so downstream channels never see collisions.
//!
//! [`TxnMap<N>`] stores a fixed number of in-flight entries backed by a
//! [`heapless::Vec`].  For serial downstream (only one in-flight request at a
//! time) use the [`SerialTxnMap`] type alias.

/// A single in-flight transaction entry.
#[derive(Debug, Clone)]
pub struct TxnEntry {
    /// Gateway-assigned internal transaction ID.
    pub internal_txn: u16,
    /// Original transaction ID from the upstream client.
    pub upstream_txn: u16,
    /// Which upstream session this transaction belongs to.
    pub session_id: u8,
}

/// Fixed-capacity transaction-ID remapping table.
///
/// # Const Generic
/// `N` — maximum number of concurrent in-flight transactions.
///
/// # Example
/// ```rust
/// use mbus_gateway::TxnMap;
///
/// let mut map: TxnMap<4> = TxnMap::new();
/// let internal = map.allocate(0x0001, 0).unwrap();
/// let entry = map.lookup(internal).unwrap();
/// assert_eq!(entry.upstream_txn, 0x0001);
/// assert_eq!(entry.session_id, 0);
/// map.remove(internal).unwrap();
/// assert!(map.is_empty());
/// ```
pub struct TxnMap<const N: usize> {
    entries: heapless::Vec<TxnEntry, N>,
    /// Monotonically-incrementing internal transaction counter.
    next_internal: u16,
}

impl<const N: usize> TxnMap<N> {
    /// Create an empty map.
    pub fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
            next_internal: 0,
        }
    }

    /// Allocate a new internal transaction ID for an upstream `(txn, session)` pair.
    ///
    /// Returns `None` if the map is full.
    pub fn allocate(&mut self, upstream_txn: u16, session_id: u8) -> Option<u16> {
        if self.entries.is_full() {
            return None;
        }
        let internal = self.next_internal;
        self.next_internal = self.next_internal.wrapping_add(1);
        self.entries
            .push(TxnEntry {
                internal_txn: internal,
                upstream_txn,
                session_id,
            })
            .ok()?;
        Some(internal)
    }

    /// Look up an entry by its internal transaction ID.
    pub fn lookup(&self, internal_txn: u16) -> Option<&TxnEntry> {
        self.entries
            .iter()
            .find(|e| e.internal_txn == internal_txn)
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
}

impl<const N: usize> Default for TxnMap<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Type alias for serial downstream where only one request can be in flight.
pub type SerialTxnMap = TxnMap<1>;
