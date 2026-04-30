//! Static pool of gateway instances for the C bindings.
//!
//! Mirrors the design of [`crate::c::client::pool`]:
//!
//! * `MbusGatewayId` is an opaque `u8` slot index (max 255).
//! * Slots use `UnsafeCell<MaybeUninit<T>>` plus an `AtomicBool` borrow flag
//!   to detect re-entrancy.
//! * External serialisation is provided by the C application via
//!   [`mbus_pool_lock`] / [`mbus_pool_unlock`] (creation/destruction) and
//!   [`mbus_gateway_lock`] / [`mbus_gateway_unlock`] (per-instance polling).
//! * RAII drop guards ensure unlock-on-panic safety.
//!
//! v1 supports a single transport variant: TCP upstream ↔ TCP downstream
//! (both using [`crate::c::transport::CTcpTransport`]). RTU / ASCII downstream
//! variants will be added in subsequent revisions following the same template.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use mbus_gateway::GatewayServices;

use crate::c::error::MbusStatusCode;
use crate::c::transport::CTcpTransport;
use crate::MAX_GATEWAYS;

use super::event_adapter::CGatewayEventAdapter;
use super::routing::CGatewayRouter;

/// Maximum number of downstream channels per gateway instance.
pub const MAX_DOWNSTREAM_CHANNELS: usize = 8;

/// Pipeline / transaction-map size for sync gateway poll cycles (always 1).
pub const TXN_SIZE: usize = 1;

/// Opaque gateway handle. `MBUS_INVALID_GATEWAY_ID` (0xFF) is the sentinel.
pub type MbusGatewayId = u8;

/// Sentinel "no valid gateway" value.
pub const MBUS_INVALID_GATEWAY_ID: MbusGatewayId = 0xFF;

/// Concrete monomorphisation of the gateway services for the v1 TCP↔TCP
/// transport variant.
pub(crate) type GatewayInner = GatewayServices<
    CTcpTransport,
    CTcpTransport,
    CGatewayRouter,
    CGatewayEventAdapter,
    MAX_DOWNSTREAM_CHANNELS,
    TXN_SIZE,
>;

// ── Extern locks (provided by the C application) ────────────────────────────

unsafe extern "C" {
    /// Global pool lock — held only during create/destroy operations. Reuses
    /// the same symbol exported by the client pool.
    fn mbus_pool_lock();
    /// Release the global pool lock.
    fn mbus_pool_unlock();
    /// Per-instance lock — held continuously during `mbus_gateway_poll`.
    fn mbus_gateway_lock(id: MbusGatewayId);
    /// Release a per-instance gateway lock.
    fn mbus_gateway_unlock(id: MbusGatewayId);
}

/// RAII guard for the global pool lock.
struct PoolLockGuard;
impl PoolLockGuard {
    fn new() -> Self {
        unsafe { mbus_pool_lock() };
        Self
    }
}
impl Drop for PoolLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_pool_unlock() };
    }
}

/// RAII guard for a per-instance gateway lock.
struct GatewayLockGuard(MbusGatewayId);
impl GatewayLockGuard {
    fn new(id: MbusGatewayId) -> Self {
        unsafe { mbus_gateway_lock(id) };
        Self(id)
    }
}
impl Drop for GatewayLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_gateway_unlock(self.0) };
    }
}

/// RAII guard that clears a borrow flag on scope exit.
struct BorrowGuard<'a>(&'a AtomicBool);
impl Drop for BorrowGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

// ── Slot ────────────────────────────────────────────────────────────────────

struct Slot {
    occupied: bool,
    value: MaybeUninit<GatewayInner>,
    borrow_flag: AtomicBool,
}

impl Slot {
    const fn empty() -> Self {
        Self {
            occupied: false,
            value: MaybeUninit::uninit(),
            borrow_flag: AtomicBool::new(false),
        }
    }
}

struct Pool {
    slots: [Slot; MAX_GATEWAYS],
}

impl Pool {
    const fn new() -> Self {
        Self {
            slots: [const { Slot::empty() }; MAX_GATEWAYS],
        }
    }

    fn allocate(&mut self, value: GatewayInner) -> Option<MbusGatewayId> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(i as MbusGatewayId);
            }
        }
        None
    }

    fn free(&mut self, id: MbusGatewayId) -> bool {
        let idx = id as usize;
        if idx >= MAX_GATEWAYS {
            return false;
        }
        let slot = &mut self.slots[idx];
        if !slot.occupied {
            return false;
        }
        unsafe { slot.value.assume_init_drop() };
        slot.borrow_flag.store(false, Ordering::SeqCst);
        slot.occupied = false;
        true
    }

    fn is_occupied(&self, id: MbusGatewayId) -> bool {
        let idx = id as usize;
        idx < MAX_GATEWAYS && self.slots[idx].occupied
    }
}

struct SyncPool(UnsafeCell<Pool>);
// SAFETY: External synchronisation is enforced via the extern "C" lock hooks
// and per-slot borrow flags.
unsafe impl Sync for SyncPool {}

static POOL: SyncPool = SyncPool(UnsafeCell::new(Pool::new()));

// ── Public pool operations ──────────────────────────────────────────────────

/// Allocate a new gateway in the pool. Returns the slot ID or
/// `MbusErrPoolFull`.
pub(super) fn pool_allocate(value: GatewayInner) -> Result<MbusGatewayId, MbusStatusCode> {
    let _guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate(value).ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Free the gateway at `id`. Returns `true` if a slot was freed.
pub(super) fn pool_free(id: MbusGatewayId) -> bool {
    if id == MBUS_INVALID_GATEWAY_ID {
        return false;
    }
    let _gw_guard = GatewayLockGuard::new(id);
    let _pool_guard = PoolLockGuard::new();
    let pool = unsafe { &mut *POOL.0.get() };
    pool.free(id)
}

/// Operate on a borrowed gateway, holding the per-instance lock and
/// detecting re-entrancy via the borrow flag.
pub(super) fn with_gateway<F, R>(id: MbusGatewayId, f: F) -> Result<R, MbusStatusCode>
where
    F: FnOnce(&mut GatewayInner) -> R,
{
    if id == MBUS_INVALID_GATEWAY_ID {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    let _gw_guard = GatewayLockGuard::new(id);
    let pool = unsafe { &mut *POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    let idx = id as usize;
    let slot = &mut pool.slots[idx];
    if slot.borrow_flag.swap(true, Ordering::SeqCst) {
        return Err(MbusStatusCode::MbusErrBusy);
    }
    let _borrow = BorrowGuard(&slot.borrow_flag);

    let inner = unsafe { slot.value.assume_init_mut() };
    Ok(f(inner))
}
