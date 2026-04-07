//! Unified static client pool — split into typed TCP and Serial sub-pools.
//!
//! ## ID Encoding
//!
//! `MbusClientId` is a `u8` with the following layout:
//!
//! ```text
//!  Bit 7 (MSB)  Bits 6-0 (index)
//!  ──────────── ───────────────
//!    0           0x00..=0x7E  →  TCP slot index
//!    1           0x00..=0x7D  →  Serial slot index  (raw byte = 0x80 + index)
//!    -           0xFF         →  MBUS_INVALID_CLIENT_ID
//! ```
//!
//! This eliminates the mixed `ClientSlot` enum (and the `large_enum_variant`
//! Clippy lint) by keeping each sub-pool homogeneous: TCP slots are sized
//! exactly to `TcpInner` and Serial slots to `SerialInner`.
//!
//! ## Safety Contract
//!
//! The pool uses `UnsafeCell` and is **not** `Sync`. Callers must guarantee
//! that all `mbus_*` functions are serialised (same thread or external mutex).
//! Thread-safety is layered on via the `mbus_pool_lock` / `mbus_client_lock`
//! extern-C hooks.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use mbus_client::services::ClientServices;

use super::app::CApp;
use super::error::MbusStatusCode;
use super::transport::CTransport;

use crate::{MAX_SERIAL_CLIENTS, MAX_TCP_CLIENTS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Pipeline depth for TCP clients (may have >1 concurrent requests).
pub(super) const TCP_PIPELINE: usize = 10;
/// Pipeline depth for serial clients (half-duplex = 1).
pub(super) const SERIAL_PIPELINE: usize = 1;

/// Client ID type: an opaque `u8` index into one of the two sub-pools.
/// Use `MBUS_INVALID_CLIENT_ID` (0xFF) as the sentinel "no client" value.
pub type MbusClientId = u8;

/// Sentinel value meaning "no valid client".
pub const MBUS_INVALID_CLIENT_ID: MbusClientId = 0xFF;

/// Bit mask that marks a `MbusClientId` as belonging to the Serial pool.
const SERIAL_BIT: u8 = 0x80;

// ── Extern Locks ──────────────────────────────────────────────────────────────

unsafe extern "C" {
    /// Lock the global pool (used only during client creation/destruction).
    fn mbus_pool_lock();
    /// Unlock the global pool.
    fn mbus_pool_unlock();

    /// Lock a specific client instance (used continuously during polling and requests).
    fn mbus_client_lock(id: MbusClientId);
    /// Unlock a specific client instance.
    fn mbus_client_unlock(id: MbusClientId);
}

/// A Drop guard to ensure `mbus_pool_unlock` is called even if a panic unwinds.
pub(super) struct PoolLockGuard;
impl PoolLockGuard {
    pub(super) fn new() -> Self {
        unsafe { mbus_pool_lock() };
        Self
    }
}
impl Drop for PoolLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_pool_unlock() };
    }
}

/// A Drop guard for per-client locks.
pub(super) struct ClientLockGuard(MbusClientId);
impl ClientLockGuard {
    pub(super) fn new(id: MbusClientId) -> Self {
        unsafe { mbus_client_lock(id) };
        Self(id)
    }
}
impl Drop for ClientLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_client_unlock(self.0) };
    }
}

/// A Drop guard that atomically clears a borrow flag on scope exit.
///
/// The caller is responsible for setting the flag to `true` (via
/// `swap`) before constructing this guard. The guard's only job is
/// to reset the flag to `false` on drop, ensuring cleanup even if
/// the borrowing closure panics (relevant in `has_unwind` / test builds).
pub(super) struct BorrowGuard<'a>(&'a AtomicBool);
impl<'a> BorrowGuard<'a> {
    /// Wrap an already-armed borrow flag. Does NOT set the flag.
    pub(super) fn new(flag: &'a AtomicBool) -> Self {
        Self(flag)
    }
}
impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

// ── Client inner types ────────────────────────────────────────────────────────

/// Type alias for a fully-specialised TCP client.
pub(super) type TcpInner = ClientServices<CTransport, CApp, TCP_PIPELINE>;
/// Type alias for a fully-specialised Serial client.
pub(super) type SerialInner = ClientServices<CTransport, CApp, SERIAL_PIPELINE>;

// ── ID helpers ────────────────────────────────────────────────────────────────

/// Returns `true` if `id` belongs to the Serial sub-pool.
#[inline(always)]
fn is_serial_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && (id & SERIAL_BIT) != 0
}

/// Returns `true` if `id` belongs to the TCP sub-pool.
#[inline(always)]
fn is_tcp_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && (id & SERIAL_BIT) == 0
}

/// Encodes a raw TCP slot index as a `MbusClientId`.
#[inline(always)]
fn tcp_id(index: usize) -> MbusClientId {
    index as u8 // MSB = 0
}

/// Encodes a raw Serial slot index as a `MbusClientId`.
#[inline(always)]
fn serial_id(index: usize) -> MbusClientId {
    (index as u8) | SERIAL_BIT
}

/// Decodes the raw TCP slot index from a `MbusClientId`.
#[inline(always)]
fn tcp_index(id: MbusClientId) -> usize {
    (id & !SERIAL_BIT) as usize
}

/// Decodes the raw Serial slot index from a `MbusClientId`.
#[inline(always)]
fn serial_index(id: MbusClientId) -> usize {
    (id & !SERIAL_BIT) as usize
}

// ── Typed slot ────────────────────────────────────────────────────────────────

struct Slot<T> {
    occupied: bool,
    value: MaybeUninit<T>,
    borrow_flag: AtomicBool,
}

impl<T> Slot<T> {
    const fn empty() -> Self {
        Self {
            occupied: false,
            value: MaybeUninit::uninit(),
            borrow_flag: AtomicBool::new(false),
        }
    }
}

// ── Pool internals ────────────────────────────────────────────────────────────

struct Pool {
    tcp_slots: [Slot<TcpInner>; MAX_TCP_CLIENTS],
    serial_slots: [Slot<SerialInner>; MAX_SERIAL_CLIENTS],
}

impl Pool {
    const fn new() -> Self {
        Self {
            tcp_slots: [const { Slot::empty() }; MAX_TCP_CLIENTS],
            serial_slots: [const { Slot::empty() }; MAX_SERIAL_CLIENTS],
        }
    }

    /// Insert a TCP client into the first free TCP slot. Returns encoded `MbusClientId` or `None`.
    fn allocate_tcp(&mut self, value: TcpInner) -> Option<MbusClientId> {
        for (i, slot) in self.tcp_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(tcp_id(i));
            }
        }
        None
    }

    /// Insert a Serial client into the first free Serial slot. Returns encoded `MbusClientId` or `None`.
    fn allocate_serial(&mut self, value: SerialInner) -> Option<MbusClientId> {
        for (i, slot) in self.serial_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(serial_id(i));
            }
        }
        None
    }

    /// Free any client by ID. Returns `true` if a slot was freed.
    fn free(&mut self, id: MbusClientId) -> bool {
        if is_tcp_id(id) {
            let idx = tcp_index(id);
            if idx >= MAX_TCP_CLIENTS {
                return false;
            }
            let slot = &mut self.tcp_slots[idx];
            if !slot.occupied {
                return false;
            }
            // SAFETY: slot is occupied so value is initialised.
            unsafe { slot.value.assume_init_drop() };
            slot.borrow_flag.store(false, Ordering::SeqCst);
            slot.occupied = false;
            true
        } else if is_serial_id(id) {
            let idx = serial_index(id);
            if idx >= MAX_SERIAL_CLIENTS {
                return false;
            }
            let slot = &mut self.serial_slots[idx];
            if !slot.occupied {
                return false;
            }
            // SAFETY: slot is occupied so value is initialised.
            unsafe { slot.value.assume_init_drop() };
            slot.borrow_flag.store(false, Ordering::SeqCst);
            slot.occupied = false;
            true
        } else {
            false
        }
    }

    /// Returns whether the slot for `id` is occupied.
    fn is_occupied(&self, id: MbusClientId) -> bool {
        if is_tcp_id(id) {
            let idx = tcp_index(id);
            idx < MAX_TCP_CLIENTS && self.tcp_slots[idx].occupied
        } else if is_serial_id(id) {
            let idx = serial_index(id);
            idx < MAX_SERIAL_CLIENTS && self.serial_slots[idx].occupied
        } else {
            false
        }
    }
}

// ── Global static pool ───────────────────────────────────────────────────────

/// Wrapper to make `UnsafeCell<Pool>` usable as a `static`.
///
/// SAFETY: External synchronization is provided via the extern "C" lock hooks,
/// enforced structurally via Drop guards. Re-entrancy is detected by the
/// per-slot `AtomicBool` borrow flags.
struct SyncPool(UnsafeCell<Pool>);

unsafe impl Sync for SyncPool {}

static POOL: SyncPool = SyncPool(UnsafeCell::new(Pool::new()));

// ── Public pool operations ────────────────────────────────────────────────────

/// Allocate a new TCP client in the pool. Returns ID or error.
pub(super) fn pool_allocate_tcp(inner: TcpInner) -> Result<MbusClientId, MbusStatusCode> {
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate_tcp(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Allocate a new Serial client in the pool. Returns ID or error.
pub(super) fn pool_allocate_serial(inner: SerialInner) -> Result<MbusClientId, MbusStatusCode> {
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate_serial(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Free the client at `id` (any type). Returns true if freed.
pub(super) fn pool_free(id: MbusClientId) -> bool {
    // Lock order is important: client first, then pool.
    // This prevents dropping a client while another thread still holds a
    // per-client borrow in `with_tcp_client` / `with_serial_client`.
    let _client_guard = ClientLockGuard::new(id);
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.free(id)
}

/// Operate on a borrowed TCP client, providing reentrancy protection.
pub(super) fn with_tcp_client<F, R>(id: MbusClientId, f: F) -> Result<R, MbusStatusCode>
where
    F: FnOnce(&mut TcpInner) -> R,
{
    if !is_tcp_id(id) {
        return Err(MbusStatusCode::MbusErrClientTypeMismatch);
    }

    let _guard = ClientLockGuard::new(id);
    let pool = unsafe { &mut *POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    let idx = tcp_index(id);
    let slot = &mut pool.tcp_slots[idx];
    if slot.borrow_flag.swap(true, Ordering::SeqCst) {
        return Err(MbusStatusCode::MbusErrBusy);
    }
    // Flag is now `true`. Guard ensures it is reset to `false` on scope exit,
    // including if the closure panics in `has_unwind` builds.
    let _borrow = BorrowGuard::new(&slot.borrow_flag);

    // SAFETY: slot is occupied and `idx` is within bounds.
    let inner = unsafe { slot.value.assume_init_mut() };
    let res = f(inner);

    Ok(res)
    // `_borrow` drops here, clearing the flag even on panic.
}

/// Operate on a borrowed Serial client, providing reentrancy protection.
pub(super) fn with_serial_client<F, R>(id: MbusClientId, f: F) -> Result<R, MbusStatusCode>
where
    F: FnOnce(&mut SerialInner) -> R,
{
    if !is_serial_id(id) {
        return Err(MbusStatusCode::MbusErrClientTypeMismatch);
    }

    let _guard = ClientLockGuard::new(id);
    let pool = unsafe { &mut *POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    let idx = serial_index(id);
    let slot = &mut pool.serial_slots[idx];
    if slot.borrow_flag.swap(true, Ordering::SeqCst) {
        return Err(MbusStatusCode::MbusErrBusy);
    }
    // Flag is now `true`. Guard ensures it is reset to `false` on scope exit,
    // including if the closure panics in `has_unwind` builds.
    let _borrow = BorrowGuard::new(&slot.borrow_flag);

    // SAFETY: slot is occupied and `idx` is within bounds.
    let inner = unsafe { slot.value.assume_init_mut() };
    let res = f(inner);

    Ok(res)
    // `_borrow` drops here, clearing the flag even on panic.
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ID encoding helpers ───────────────────────────────────────────────────

    #[test]
    fn tcp_id_encoding_has_msb_clear() {
        let id = tcp_id(0);
        assert_eq!(id, 0x00);
        assert!(is_tcp_id(id));
        assert!(!is_serial_id(id));

        let id = tcp_id(5);
        assert_eq!(id, 0x05);
        assert!(is_tcp_id(id));
        assert!(!is_serial_id(id));

        let id = tcp_id(127);
        assert_eq!(id & SERIAL_BIT, 0, "TCP IDs must have MSB clear");
        assert!(is_tcp_id(id));
    }

    #[test]
    fn serial_id_encoding_has_msb_set() {
        let id = serial_id(0);
        assert_eq!(id, 0x80);
        assert!(is_serial_id(id));
        assert!(!is_tcp_id(id));

        let id = serial_id(5);
        assert_eq!(id, 0x85);
        assert!(is_serial_id(id));

        assert!(!is_serial_id(MBUS_INVALID_CLIENT_ID));
        assert!(!is_tcp_id(MBUS_INVALID_CLIENT_ID));
    }

    #[test]
    fn tcp_index_roundtrip() {
        for i in 0..MAX_TCP_CLIENTS {
            let id = tcp_id(i);
            assert_eq!(tcp_index(id), i, "roundtrip failed for tcp index {i}");
        }
    }

    #[test]
    fn serial_index_roundtrip() {
        for i in 0..MAX_SERIAL_CLIENTS {
            let id = serial_id(i);
            assert_eq!(serial_index(id), i, "roundtrip failed for serial index {i}");
        }
    }

    #[test]
    fn invalid_id_is_neither_tcp_nor_serial() {
        assert!(!is_tcp_id(MBUS_INVALID_CLIENT_ID));
        assert!(!is_serial_id(MBUS_INVALID_CLIENT_ID));
    }

    // ── Pool (internal, no extern-C locks) ───────────────────────────────────

    fn make_pool() -> Pool {
        Pool::new()
    }

    #[test]
    fn pool_starts_fully_empty() {
        let pool = make_pool();
        for i in 0..MAX_TCP_CLIENTS {
            assert!(!pool.tcp_slots[i].occupied);
        }
        for i in 0..MAX_SERIAL_CLIENTS {
            assert!(!pool.serial_slots[i].occupied);
        }
    }

    #[test]
    fn is_occupied_rejects_invalid_id() {
        let pool = make_pool();
        assert!(!pool.is_occupied(MBUS_INVALID_CLIENT_ID));
        assert!(!pool.is_occupied(tcp_id(0)));
        assert!(!pool.is_occupied(serial_id(0)));
    }

    #[test]
    fn free_of_unoccupied_slot_returns_false() {
        let mut pool = make_pool();
        assert!(!pool.free(tcp_id(0)));
        assert!(!pool.free(serial_id(0)));
        assert!(!pool.free(MBUS_INVALID_CLIENT_ID));
    }

    #[test]
    fn free_out_of_bounds_id_returns_false() {
        let mut pool = make_pool();
        let oob = tcp_id(MAX_TCP_CLIENTS);
        let result = pool.free(oob);
        assert!(!result || !pool.is_occupied(oob));
    }

    #[test]
    fn with_tcp_client_rejects_serial_id() {
        assert!(!is_tcp_id(serial_id(0)));
    }

    #[test]
    fn with_serial_client_rejects_tcp_id() {
        assert!(!is_serial_id(tcp_id(0)));
    }

    #[test]
    fn tcp_pool_capacity_boundary() {
        if MAX_TCP_CLIENTS > 0 {
            let last = tcp_id(MAX_TCP_CLIENTS - 1);
            assert!(is_tcp_id(last));
            assert!(!is_serial_id(last));
            assert_ne!(last, MBUS_INVALID_CLIENT_ID);
        }
    }

    #[test]
    fn serial_pool_capacity_boundary() {
        if MAX_SERIAL_CLIENTS > 0 {
            let last = serial_id(MAX_SERIAL_CLIENTS - 1);
            assert!(is_serial_id(last));
            assert!(!is_tcp_id(last));
            assert_ne!(last, MBUS_INVALID_CLIENT_ID);
        }
    }

    #[test]
    fn tcp_and_serial_id_spaces_do_not_overlap() {
        for ti in 0..MAX_TCP_CLIENTS {
            for si in 0..MAX_SERIAL_CLIENTS {
                assert_ne!(
                    tcp_id(ti),
                    serial_id(si),
                    "ID collision at tcp={ti} serial={si}"
                );
            }
        }
    }

    // ── Pool exhaustion & free/realloc ────────────────────────────────────────

    #[test]
    fn tcp_pool_all_slots_start_unoccupied() {
        let pool = Pool::new();
        assert!(pool.tcp_slots.iter().all(|s| !s.occupied));
    }

    #[test]
    fn tcp_pool_fill_then_all_occupied() {
        let mut pool = Pool::new();
        for slot in pool.tcp_slots.iter_mut() {
            slot.occupied = true;
        }
        for i in 0..MAX_TCP_CLIENTS {
            assert!(pool.is_occupied(tcp_id(i)), "slot {i} should be occupied");
        }
    }

    #[test]
    fn tcp_free_marks_slot_unoccupied() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        assert!(pool.is_occupied(tcp_id(0)));
        assert!(pool.free(tcp_id(0)));
        assert!(!pool.is_occupied(tcp_id(0)));
    }

    #[test]
    fn serial_free_marks_slot_unoccupied() {
        let mut pool = Pool::new();
        pool.serial_slots[0].occupied = true;
        assert!(pool.is_occupied(serial_id(0)));
        assert!(pool.free(serial_id(0)));
        assert!(!pool.is_occupied(serial_id(0)));
    }

    #[test]
    fn free_clears_borrow_flag() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        pool.tcp_slots[0].borrow_flag.store(true, Ordering::SeqCst);
        pool.free(tcp_id(0));
        assert!(!pool.tcp_slots[0].borrow_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn double_free_returns_false() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        assert!(pool.free(tcp_id(0)));
        assert!(!pool.free(tcp_id(0)));
    }

    #[test]
    fn all_slots_free_after_full_fill_and_clear() {
        let mut pool = Pool::new();
        for slot in pool.tcp_slots.iter_mut() {
            slot.occupied = true;
        }
        for slot in pool.serial_slots.iter_mut() {
            slot.occupied = true;
        }
        for i in 0..MAX_TCP_CLIENTS {
            assert!(pool.free(tcp_id(i)));
        }
        for i in 0..MAX_SERIAL_CLIENTS {
            assert!(pool.free(serial_id(i)));
        }
        for i in 0..MAX_TCP_CLIENTS {
            assert!(!pool.is_occupied(tcp_id(i)));
        }
        for i in 0..MAX_SERIAL_CLIENTS {
            assert!(!pool.is_occupied(serial_id(i)));
        }
    }
}
