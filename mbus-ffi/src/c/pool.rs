//! Unified static client pool.
//!
//! Provides a fixed-capacity pool of Modbus client slots (TCP or Serial).
//! C users never receive a Rust pointer — they operate via a numeric
//! [`MbusClientId`] that indexes into this pool.
//!
//! # Safety Contract (Phase 1 — single-threaded)
//!
//! The pool uses `UnsafeCell` and is **not** `Sync`. Callers must guarantee
//! that all `mbus_*` functions are called from the same thread (or are
//! externally serialised). Thread-safety will be layered on in a future phase.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;

use mbus_client::services::ClientServices;

use super::app::CApp;
use super::error::MbusStatusCode;
use super::transport::CTransport;

use crate::MAX_CLIENTS;

// ── Constants ─────────────────────────────────────────────────────────────────

/// Pipeline depth for TCP clients (may have >1 concurrent requests).
pub(super) const TCP_PIPELINE: usize = 10;
/// Pipeline depth for serial clients (half-duplex = 1).
pub(super) const SERIAL_PIPELINE: usize = 1;

/// Client ID type: index into the static pool.
///
/// `MBUS_INVALID_CLIENT_ID` (0xFF) indicates failure.
pub type MbusClientId = u8;

/// Sentinel value meaning "no valid client".
pub const MBUS_INVALID_CLIENT_ID: MbusClientId = 0xFF;

// ── Client inner types ────────────────────────────────────────────────────────

/// Type alias for a fully-specialised TCP client.
pub(super) type TcpInner = ClientServices<CTransport, CApp, TCP_PIPELINE>;
/// Type alias for a fully-specialised Serial client.
pub(super) type SerialInner = ClientServices<CTransport, CApp, SERIAL_PIPELINE>;

/// A pool slot can hold either variant.
pub(super) enum ClientSlot {
    /// A Modbus TCP client.
    Tcp(TcpInner),
    /// A Modbus Serial (RTU / ASCII) client.
    Serial(SerialInner),
}

// ── Pool internals ────────────────────────────────────────────────────────────

struct Slot {
    occupied: bool,
    value: MaybeUninit<ClientSlot>,
}

impl Slot {
    const EMPTY: Self = Self {
        occupied: false,
        value: MaybeUninit::uninit(),
    };
}

struct Pool {
    slots: [Slot; MAX_CLIENTS],
}

impl Pool {
    const fn new() -> Self {
        Self {
            slots: [const { Slot::EMPTY }; MAX_CLIENTS],
        }
    }

    /// Insert `value` into the first free slot. Returns the slot index or `None`.
    fn allocate(&mut self, value: ClientSlot) -> Option<MbusClientId> {
        for (i, slot) in self.slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.occupied = true;
                return Some(i as MbusClientId);
            }
        }
        None
    }

    /// Free the slot at `id`, dropping the contained client.
    fn free(&mut self, id: MbusClientId) -> bool {
        let idx = id as usize;
        if idx >= MAX_CLIENTS {
            return false;
        }
        let slot = &mut self.slots[idx];
        if !slot.occupied {
            return false;
        }
        // SAFETY: slot is occupied so value is initialised.
        unsafe { slot.value.assume_init_drop() };
        slot.occupied = false;
        true
    }

    /// Return a mutable reference to a TCP client at `id`, or `None`.
    fn get_tcp_mut(&mut self, id: MbusClientId) -> Option<&mut TcpInner> {
        let idx = id as usize;
        if idx >= MAX_CLIENTS {
            return None;
        }
        let slot = &mut self.slots[idx];
        if !slot.occupied {
            return None;
        }
        // SAFETY: slot is occupied.
        let client = unsafe { slot.value.assume_init_mut() };
        match client {
            ClientSlot::Tcp(inner) => Some(inner),
            ClientSlot::Serial(_) => None,
        }
    }

    /// Return a mutable reference to a Serial client at `id`, or `None`.
    fn get_serial_mut(&mut self, id: MbusClientId) -> Option<&mut SerialInner> {
        let idx = id as usize;
        if idx >= MAX_CLIENTS {
            return None;
        }
        let slot = &mut self.slots[idx];
        if !slot.occupied {
            return None;
        }
        // SAFETY: slot is occupied.
        let client = unsafe { slot.value.assume_init_mut() };
        match client {
            ClientSlot::Serial(inner) => Some(inner),
            ClientSlot::Tcp(_) => None,
        }
    }

    /// Return whether a slot is occupied (for type-agnostic free).
    fn is_occupied(&self, id: MbusClientId) -> bool {
        let idx = id as usize;
        if idx >= MAX_CLIENTS {
            return false;
        }
        self.slots[idx].occupied
    }
}

// ── Global static pool ───────────────────────────────────────────────────────

/// Wrapper to make `UnsafeCell<Pool>` usable as a `static`.
///
/// SAFETY: Phase 1 single-threaded contract. The C user guarantees all
/// `mbus_*` calls are serialised externally (single thread or external mutex).
struct SyncPool(UnsafeCell<Pool>);

// SAFETY: single-threaded contract — see module-level doc comment.
unsafe impl Sync for SyncPool {}

static POOL: SyncPool = SyncPool(UnsafeCell::new(Pool::new()));


// ── Public pool operations (used by tcp_client.rs / serial_client.rs) ────────

/// Allocate a new TCP client in the pool. Returns ID or error.
pub(super) fn pool_allocate_tcp(inner: TcpInner) -> Result<MbusClientId, MbusStatusCode> {
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate(ClientSlot::Tcp(inner))
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Allocate a new Serial client in the pool. Returns ID or error.
pub(super) fn pool_allocate_serial(inner: SerialInner) -> Result<MbusClientId, MbusStatusCode> {
    let pool = unsafe { &mut *POOL.0.get() };
    pool.allocate(ClientSlot::Serial(inner))
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

/// Free the client at `id` (any type). Returns true if freed.
pub(super) fn pool_free(id: MbusClientId) -> bool {
    let pool = unsafe { &mut *POOL.0.get() };
    pool.free(id)
}

/// Get a mutable reference to the TCP client at `id`, or an error code.
pub(super) fn pool_get_tcp(id: MbusClientId) -> Result<&'static mut TcpInner, MbusStatusCode> {
    let pool = unsafe { &mut *POOL.0.get() };
    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }
    pool.get_tcp_mut(id).ok_or(MbusStatusCode::MbusErrClientTypeMismatch)
}

/// Get a mutable reference to the Serial client at `id`, or an error code.
pub(super) fn pool_get_serial(id: MbusClientId) -> Result<&'static mut SerialInner, MbusStatusCode> {
    let pool = unsafe { &mut *POOL.0.get() };
    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }
    pool.get_serial_mut(id).ok_or(MbusStatusCode::MbusErrClientTypeMismatch)
}
