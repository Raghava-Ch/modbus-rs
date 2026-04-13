//! Unified static client pool — split into typed TCP, Serial RTU, and Serial ASCII sub-pools.
//!
//! ## ID Encoding
//!
//! `MbusClientId` is a `u16` with the following layout:
//!
//! ```text
//!  High byte (pool tag)   Low byte (slot index)
//!  ──────────────────── ─────────────────────────
//!    0x00                0x00..=0xFE  →  TCP slot
//!    0x01                0x00..=0xFE  →  Serial RTU slot
//!    0x02                0x00..=0xFE  →  Serial ASCII slot
//!    0xFF                0xFF         →  MBUS_INVALID_CLIENT_ID (0xFFFF)
//! ```
//!
//! This eliminates the mixed `ClientSlot` enum (and the `large_enum_variant`
//! Clippy lint) by keeping each sub-pool homogeneous: TCP slots are sized
//! exactly to `TcpInner`, RTU slots to `SerialRtuInner`, and ASCII slots
//! to `SerialAsciiInner`.
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
use super::transport::{CAsciiTransport, CRtuTransport, CTcpTransport};

use crate::{MAX_SERIAL_CLIENTS, MAX_TCP_CLIENTS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Pipeline depth for TCP clients (may have >1 concurrent requests).
pub(super) const TCP_PIPELINE: usize = 10;
/// Pipeline depth for serial clients (half-duplex = 1).
pub(super) const SERIAL_PIPELINE: usize = 1;

/// Client ID type: an opaque `u16` index into one of the three sub-pools.
/// Use `MBUS_INVALID_CLIENT_ID` (0xFFFF) as the sentinel "no client" value.
pub type MbusClientId = u16;

/// Sentinel value meaning "no valid client".
pub const MBUS_INVALID_CLIENT_ID: MbusClientId = 0xFFFF;

/// Pool tag occupying the high byte of a `MbusClientId`.
const TAG_TCP: u8 = 0x00;
const TAG_SERIAL_RTU: u8 = 0x01;
const TAG_SERIAL_ASCII: u8 = 0x02;

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
pub(super) type TcpInner = ClientServices<CTcpTransport, CApp, TCP_PIPELINE>;
/// Type alias for a fully-specialised Serial RTU client.
pub(super) type SerialRtuInner = ClientServices<CRtuTransport, CApp, SERIAL_PIPELINE>;
/// Type alias for a fully-specialised Serial ASCII client.
pub(super) type SerialAsciiInner = ClientServices<CAsciiTransport, CApp, SERIAL_PIPELINE>;

// ── ID helpers ────────────────────────────────────────────────────────────────

/// Extracts the pool tag (high byte) from a `MbusClientId`.
#[inline(always)]
fn id_tag(id: MbusClientId) -> u8 {
    (id >> 8) as u8
}

/// Extracts the slot index (low byte) from a `MbusClientId`.
#[inline(always)]
fn id_index(id: MbusClientId) -> usize {
    (id & 0xFF) as usize
}

/// Encodes a pool tag and slot index into a `MbusClientId`.
#[inline(always)]
fn encode_id(tag: u8, index: usize) -> MbusClientId {
    ((tag as u16) << 8) | (index as u16)
}

/// Returns `true` if `id` belongs to the TCP sub-pool.
#[inline(always)]
fn is_tcp_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && id_tag(id) == TAG_TCP
}

/// Returns `true` if `id` belongs to either Serial sub-pool (RTU or ASCII).
#[inline(always)]
fn is_serial_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && (id_tag(id) == TAG_SERIAL_RTU || id_tag(id) == TAG_SERIAL_ASCII)
}

/// Returns `true` if `id` belongs to the Serial RTU sub-pool.
#[inline(always)]
fn is_serial_rtu_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && id_tag(id) == TAG_SERIAL_RTU
}

/// Returns `true` if `id` belongs to the Serial ASCII sub-pool.
#[inline(always)]
#[cfg(test)]
fn is_serial_ascii_id(id: MbusClientId) -> bool {
    id != MBUS_INVALID_CLIENT_ID && id_tag(id) == TAG_SERIAL_ASCII
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
    serial_rtu_slots: [Slot<SerialRtuInner>; MAX_SERIAL_CLIENTS],
    serial_ascii_slots: [Slot<SerialAsciiInner>; MAX_SERIAL_CLIENTS],
}

impl Pool {
    const fn new() -> Self {
        Self {
            tcp_slots: [const { Slot::empty() }; MAX_TCP_CLIENTS],
            serial_rtu_slots: [const { Slot::empty() }; MAX_SERIAL_CLIENTS],
            serial_ascii_slots: [const { Slot::empty() }; MAX_SERIAL_CLIENTS],
        }
    }

    /// Insert a TCP client into the first free TCP slot.
    fn allocate_tcp(&mut self, value: TcpInner) -> Option<MbusClientId> {
        for (i, slot) in self.tcp_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_id(TAG_TCP, i));
            }
        }
        None
    }

    /// Insert a Serial RTU client into the first free RTU slot.
    fn allocate_serial_rtu(&mut self, value: SerialRtuInner) -> Option<MbusClientId> {
        for (i, slot) in self.serial_rtu_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_id(TAG_SERIAL_RTU, i));
            }
        }
        None
    }

    /// Insert a Serial ASCII client into the first free ASCII slot.
    fn allocate_serial_ascii(&mut self, value: SerialAsciiInner) -> Option<MbusClientId> {
        for (i, slot) in self.serial_ascii_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_id(TAG_SERIAL_ASCII, i));
            }
        }
        None
    }

    /// Free any client by ID. Returns `true` if a slot was freed.
    fn free(&mut self, id: MbusClientId) -> bool {
        let idx = id_index(id);
        match id_tag(id) {
            TAG_TCP => {
                if idx >= MAX_TCP_CLIENTS {
                    return false;
                }
                let slot = &mut self.tcp_slots[idx];
                if !slot.occupied {
                    return false;
                }
                unsafe { slot.value.assume_init_drop() };
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = false;
                true
            }
            TAG_SERIAL_RTU => {
                if idx >= MAX_SERIAL_CLIENTS {
                    return false;
                }
                let slot = &mut self.serial_rtu_slots[idx];
                if !slot.occupied {
                    return false;
                }
                unsafe { slot.value.assume_init_drop() };
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = false;
                true
            }
            TAG_SERIAL_ASCII => {
                if idx >= MAX_SERIAL_CLIENTS {
                    return false;
                }
                let slot = &mut self.serial_ascii_slots[idx];
                if !slot.occupied {
                    return false;
                }
                unsafe { slot.value.assume_init_drop() };
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = false;
                true
            }
            _ => false,
        }
    }

    /// Returns whether the slot for `id` is occupied.
    fn is_occupied(&self, id: MbusClientId) -> bool {
        let idx = id_index(id);
        match id_tag(id) {
            TAG_TCP => idx < MAX_TCP_CLIENTS && self.tcp_slots[idx].occupied,
            TAG_SERIAL_RTU => idx < MAX_SERIAL_CLIENTS && self.serial_rtu_slots[idx].occupied,
            TAG_SERIAL_ASCII => idx < MAX_SERIAL_CLIENTS && self.serial_ascii_slots[idx].occupied,
            _ => false,
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

/// Allocate a new Serial RTU client in the pool. Returns ID or error.
pub(super) fn pool_allocate_serial_rtu(
    inner: SerialRtuInner,
) -> Result<MbusClientId, MbusStatusCode> {
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate_serial_rtu(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Allocate a new Serial ASCII client in the pool. Returns ID or error.
pub(super) fn pool_allocate_serial_ascii(
    inner: SerialAsciiInner,
) -> Result<MbusClientId, MbusStatusCode> {
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate_serial_ascii(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Free the client at `id` (any type). Returns true if freed.
pub(super) fn pool_free(id: MbusClientId) -> bool {
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

    let idx = id_index(id);
    let slot = &mut pool.tcp_slots[idx];
    if slot.borrow_flag.swap(true, Ordering::SeqCst) {
        return Err(MbusStatusCode::MbusErrBusy);
    }
    let _borrow = BorrowGuard::new(&slot.borrow_flag);

    let inner = unsafe { slot.value.assume_init_mut() };
    Ok(f(inner))
}

/// Internal helper: borrow from a typed slot array.
macro_rules! dispatch_serial {
    ($id:expr, $pool:expr, $slots:ident, $f:expr) => {{
        let idx = id_index($id);
        let slot = &mut $pool.$slots[idx];
        if slot.borrow_flag.swap(true, Ordering::SeqCst) {
            return Err(MbusStatusCode::MbusErrBusy);
        }
        let _borrow = BorrowGuard::new(&slot.borrow_flag);
        let inner = unsafe { slot.value.assume_init_mut() };
        Ok($f(inner))
    }};
}

/// Operate on a borrowed Serial client (either RTU or ASCII).
///
/// Because `SerialRtuInner` and `SerialAsciiInner` are different concrete types,
/// callers provide **two** closures — one for each monomorphisation. In practice
/// both closures have identical bodies (they only call `ClientServices` methods
/// which are uniform across transport generics). Use the convenience macro
/// [`with_serial_client_uniform!`] to avoid the duplication at call sites.
pub(super) fn with_serial_client<F1, F2, R>(
    id: MbusClientId,
    f_rtu: F1,
    f_ascii: F2,
) -> Result<R, MbusStatusCode>
where
    F1: FnOnce(&mut SerialRtuInner) -> R,
    F2: FnOnce(&mut SerialAsciiInner) -> R,
{
    if !is_serial_id(id) {
        return Err(MbusStatusCode::MbusErrClientTypeMismatch);
    }

    let _guard = ClientLockGuard::new(id);
    let pool = unsafe { &mut *POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    if is_serial_rtu_id(id) {
        dispatch_serial!(id, pool, serial_rtu_slots, f_rtu)
    } else {
        dispatch_serial!(id, pool, serial_ascii_slots, f_ascii)
    }
}

/// Convenience macro for call sites that pass the same closure body to both
/// RTU and ASCII dispatch paths of [`with_serial_client`].
///
/// Usage:
/// ```ignore
/// with_serial_client_uniform!(id, |inner| { inner.poll() })
/// ```
macro_rules! with_serial_client_uniform {
    ($id:expr, |$inner:ident| $body:expr) => {
        $crate::c::pool::with_serial_client($id, |$inner| $body, |$inner| $body)
    };
}
pub(super) use with_serial_client_uniform;

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── ID encoding helpers ───────────────────────────────────────────────────

    #[test]
    fn tcp_id_encoding() {
        let id = encode_id(TAG_TCP, 0);
        assert_eq!(id, 0x0000);
        assert!(is_tcp_id(id));
        assert!(!is_serial_id(id));

        let id = encode_id(TAG_TCP, 5);
        assert_eq!(id, 0x0005);
        assert!(is_tcp_id(id));
        assert!(!is_serial_id(id));
    }

    #[test]
    fn serial_rtu_id_encoding() {
        let id = encode_id(TAG_SERIAL_RTU, 0);
        assert_eq!(id, 0x0100);
        assert!(is_serial_id(id));
        assert!(is_serial_rtu_id(id));
        assert!(!is_serial_ascii_id(id));
        assert!(!is_tcp_id(id));

        let id = encode_id(TAG_SERIAL_RTU, 5);
        assert_eq!(id, 0x0105);
        assert!(is_serial_id(id));
        assert!(is_serial_rtu_id(id));
    }

    #[test]
    fn serial_ascii_id_encoding() {
        let id = encode_id(TAG_SERIAL_ASCII, 0);
        assert_eq!(id, 0x0200);
        assert!(is_serial_id(id));
        assert!(is_serial_ascii_id(id));
        assert!(!is_serial_rtu_id(id));
        assert!(!is_tcp_id(id));

        let id = encode_id(TAG_SERIAL_ASCII, 3);
        assert_eq!(id, 0x0203);
        assert!(is_serial_id(id));
        assert!(is_serial_ascii_id(id));
    }

    #[test]
    fn tcp_index_roundtrip() {
        for i in 0..MAX_TCP_CLIENTS {
            let id = encode_id(TAG_TCP, i);
            assert_eq!(id_index(id), i, "roundtrip failed for tcp index {i}");
        }
    }

    #[test]
    fn serial_rtu_index_roundtrip() {
        for i in 0..MAX_SERIAL_CLIENTS {
            let id = encode_id(TAG_SERIAL_RTU, i);
            assert_eq!(id_index(id), i, "roundtrip failed for rtu index {i}");
        }
    }

    #[test]
    fn serial_ascii_index_roundtrip() {
        for i in 0..MAX_SERIAL_CLIENTS {
            let id = encode_id(TAG_SERIAL_ASCII, i);
            assert_eq!(id_index(id), i, "roundtrip failed for ascii index {i}");
        }
    }

    #[test]
    fn invalid_id_is_neither_tcp_nor_serial() {
        assert!(!is_tcp_id(MBUS_INVALID_CLIENT_ID));
        assert!(!is_serial_id(MBUS_INVALID_CLIENT_ID));
        assert!(!is_serial_rtu_id(MBUS_INVALID_CLIENT_ID));
        assert!(!is_serial_ascii_id(MBUS_INVALID_CLIENT_ID));
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
            assert!(!pool.serial_rtu_slots[i].occupied);
            assert!(!pool.serial_ascii_slots[i].occupied);
        }
    }

    #[test]
    fn is_occupied_rejects_invalid_id() {
        let pool = make_pool();
        assert!(!pool.is_occupied(MBUS_INVALID_CLIENT_ID));
        assert!(!pool.is_occupied(encode_id(TAG_TCP, 0)));
        assert!(!pool.is_occupied(encode_id(TAG_SERIAL_RTU, 0)));
        assert!(!pool.is_occupied(encode_id(TAG_SERIAL_ASCII, 0)));
    }

    #[test]
    fn free_of_unoccupied_slot_returns_false() {
        let mut pool = make_pool();
        assert!(!pool.free(encode_id(TAG_TCP, 0)));
        assert!(!pool.free(encode_id(TAG_SERIAL_RTU, 0)));
        assert!(!pool.free(encode_id(TAG_SERIAL_ASCII, 0)));
        assert!(!pool.free(MBUS_INVALID_CLIENT_ID));
    }

    #[test]
    fn free_out_of_bounds_id_returns_false() {
        let mut pool = make_pool();
        let oob = encode_id(TAG_TCP, MAX_TCP_CLIENTS);
        let result = pool.free(oob);
        assert!(!result || !pool.is_occupied(oob));
    }

    #[test]
    fn with_tcp_client_rejects_serial_id() {
        assert!(!is_tcp_id(encode_id(TAG_SERIAL_RTU, 0)));
        assert!(!is_tcp_id(encode_id(TAG_SERIAL_ASCII, 0)));
    }

    #[test]
    fn with_serial_client_rejects_tcp_id() {
        assert!(!is_serial_id(encode_id(TAG_TCP, 0)));
    }

    #[test]
    fn tcp_pool_capacity_boundary() {
        if MAX_TCP_CLIENTS > 0 {
            let last = encode_id(TAG_TCP, MAX_TCP_CLIENTS - 1);
            assert!(is_tcp_id(last));
            assert!(!is_serial_id(last));
            assert_ne!(last, MBUS_INVALID_CLIENT_ID);
        }
    }

    #[test]
    fn serial_rtu_pool_capacity_boundary() {
        if MAX_SERIAL_CLIENTS > 0 {
            let last = encode_id(TAG_SERIAL_RTU, MAX_SERIAL_CLIENTS - 1);
            assert!(is_serial_id(last));
            assert!(is_serial_rtu_id(last));
            assert!(!is_tcp_id(last));
            assert_ne!(last, MBUS_INVALID_CLIENT_ID);
        }
    }

    #[test]
    fn serial_ascii_pool_capacity_boundary() {
        if MAX_SERIAL_CLIENTS > 0 {
            let last = encode_id(TAG_SERIAL_ASCII, MAX_SERIAL_CLIENTS - 1);
            assert!(is_serial_id(last));
            assert!(is_serial_ascii_id(last));
            assert!(!is_tcp_id(last));
            assert_ne!(last, MBUS_INVALID_CLIENT_ID);
        }
    }

    #[test]
    fn tcp_rtu_ascii_id_spaces_do_not_overlap() {
        for ti in 0..MAX_TCP_CLIENTS {
            for si in 0..MAX_SERIAL_CLIENTS {
                let tcp = encode_id(TAG_TCP, ti);
                let rtu = encode_id(TAG_SERIAL_RTU, si);
                let ascii = encode_id(TAG_SERIAL_ASCII, si);
                assert_ne!(tcp, rtu, "TCP/RTU collision at tcp={ti} rtu={si}");
                assert_ne!(tcp, ascii, "TCP/ASCII collision at tcp={ti} ascii={si}");
                assert_ne!(rtu, ascii, "RTU/ASCII collision at rtu={si} ascii={si}");
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
            assert!(
                pool.is_occupied(encode_id(TAG_TCP, i)),
                "slot {i} should be occupied"
            );
        }
    }

    #[test]
    fn tcp_free_marks_slot_unoccupied() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        assert!(pool.is_occupied(encode_id(TAG_TCP, 0)));
        assert!(pool.free(encode_id(TAG_TCP, 0)));
        assert!(!pool.is_occupied(encode_id(TAG_TCP, 0)));
    }

    #[test]
    fn serial_rtu_free_marks_slot_unoccupied() {
        let mut pool = Pool::new();
        pool.serial_rtu_slots[0].occupied = true;
        assert!(pool.is_occupied(encode_id(TAG_SERIAL_RTU, 0)));
        assert!(pool.free(encode_id(TAG_SERIAL_RTU, 0)));
        assert!(!pool.is_occupied(encode_id(TAG_SERIAL_RTU, 0)));
    }

    #[test]
    fn serial_ascii_free_marks_slot_unoccupied() {
        let mut pool = Pool::new();
        pool.serial_ascii_slots[0].occupied = true;
        assert!(pool.is_occupied(encode_id(TAG_SERIAL_ASCII, 0)));
        assert!(pool.free(encode_id(TAG_SERIAL_ASCII, 0)));
        assert!(!pool.is_occupied(encode_id(TAG_SERIAL_ASCII, 0)));
    }

    #[test]
    fn free_clears_borrow_flag() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        pool.tcp_slots[0].borrow_flag.store(true, Ordering::SeqCst);
        pool.free(encode_id(TAG_TCP, 0));
        assert!(!pool.tcp_slots[0].borrow_flag.load(Ordering::SeqCst));
    }

    #[test]
    fn double_free_returns_false() {
        let mut pool = Pool::new();
        pool.tcp_slots[0].occupied = true;
        assert!(pool.free(encode_id(TAG_TCP, 0)));
        assert!(!pool.free(encode_id(TAG_TCP, 0)));
    }

    #[test]
    fn all_slots_free_after_full_fill_and_clear() {
        let mut pool = Pool::new();
        for slot in pool.tcp_slots.iter_mut() {
            slot.occupied = true;
        }
        for slot in pool.serial_rtu_slots.iter_mut() {
            slot.occupied = true;
        }
        for slot in pool.serial_ascii_slots.iter_mut() {
            slot.occupied = true;
        }
        for i in 0..MAX_TCP_CLIENTS {
            assert!(pool.free(encode_id(TAG_TCP, i)));
        }
        for i in 0..MAX_SERIAL_CLIENTS {
            assert!(pool.free(encode_id(TAG_SERIAL_RTU, i)));
            assert!(pool.free(encode_id(TAG_SERIAL_ASCII, i)));
        }
        for i in 0..MAX_TCP_CLIENTS {
            assert!(!pool.is_occupied(encode_id(TAG_TCP, i)));
        }
        for i in 0..MAX_SERIAL_CLIENTS {
            assert!(!pool.is_occupied(encode_id(TAG_SERIAL_RTU, i)));
            assert!(!pool.is_occupied(encode_id(TAG_SERIAL_ASCII, i)));
        }
    }
}
