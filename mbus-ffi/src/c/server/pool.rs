//! Static server pool — mirrors the client pool design in `super::super::client::pool`.
//!
//! ## ID Encoding
//!
//! `MbusServerId` is a `u16` with the following layout:
//!
//! ```text
//!  High byte (pool tag)   Low byte (slot index)
//!  ──────────────────── ─────────────────────────
//!    0x10                0x00..=0xFE  →  TCP server slot
//!    0x11                0x00..=0xFE  →  Serial RTU server slot
//!    0xFF                0xFF         →  MBUS_INVALID_SERVER_ID (0xFFFF)
//! ```
//!
//! ## Safety Contract
//!
//! Same as the client pool: `UnsafeCell` + external locking via `mbus_server_pool_lock` /
//! `mbus_server_pool_unlock` hooks for pool-level operations, and `mbus_server_lock` /
//! `mbus_server_unlock` hooks for per-server operations.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicBool, Ordering};

use mbus_server::ServerServices;

use super::app::CServerApp;
use crate::c::error::MbusStatusCode;
use crate::c::transport::{CAsciiTransport, CRtuTransport, CTcpTransport};
use crate::{MAX_SERIAL_SERVERS, MAX_TCP_SERVERS};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Queue depth (max concurrent in-flight requests) for TCP servers.
pub(super) const SERVER_TCP_QUEUE_DEPTH: usize = 8;
/// Queue depth for serial servers (half-duplex = 1).
pub(super) const SERVER_SERIAL_QUEUE_DEPTH: usize = 1;

/// Server ID type: an opaque `u16` index into one of the server sub-pools.
pub type MbusServerId = u16;

/// Sentinel value meaning "no valid server" / creation failed.
pub const MBUS_INVALID_SERVER_ID: MbusServerId = 0xFFFF;

/// Pool tag for TCP servers.
const TAG_TCP_SERVER: u8 = 0x10;
/// Pool tag for Serial RTU servers.
const TAG_SERIAL_RTU_SERVER: u8 = 0x11;
/// Pool tag for Serial ASCII servers.
const TAG_SERIAL_ASCII_SERVER: u8 = 0x12;

// ── Extern locks ──────────────────────────────────────────────────────────────

unsafe extern "C" {
    /// Lock the global server pool (used only during server creation/destruction).
    fn mbus_server_pool_lock();
    /// Unlock the global server pool.
    fn mbus_server_pool_unlock();

    /// Lock a specific server instance.
    fn mbus_server_lock(id: MbusServerId);
    /// Unlock a specific server instance.
    fn mbus_server_unlock(id: MbusServerId);
}

/// RAII guard for the server pool lock.
pub(super) struct ServerPoolLockGuard;
impl ServerPoolLockGuard {
    pub(super) fn new() -> Self {
        unsafe { mbus_server_pool_lock() };
        Self
    }
}
impl Drop for ServerPoolLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_server_pool_unlock() };
    }
}

/// RAII guard for a per-server lock.
pub(super) struct ServerLockGuard(MbusServerId);
impl ServerLockGuard {
    pub(super) fn new(id: MbusServerId) -> Self {
        unsafe { mbus_server_lock(id) };
        Self(id)
    }
}
impl Drop for ServerLockGuard {
    fn drop(&mut self) {
        unsafe { mbus_server_unlock(self.0) };
    }
}

/// RAII guard that clears a borrow flag on drop.
pub(super) struct ServerBorrowGuard<'a>(&'a AtomicBool);
impl<'a> ServerBorrowGuard<'a> {
    pub(super) fn new(flag: &'a AtomicBool) -> Self {
        Self(flag)
    }
}
impl Drop for ServerBorrowGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

// ── Server inner types ────────────────────────────────────────────────────────

/// Fully-specialised TCP server type stored in the pool.
pub(super) type TcpServerInner =
    ServerServices<CTcpTransport, CServerApp, SERVER_TCP_QUEUE_DEPTH>;
/// Fully-specialised Serial RTU server type.
pub(super) type SerialRtuServerInner =
    ServerServices<CRtuTransport, CServerApp, SERVER_SERIAL_QUEUE_DEPTH>;
/// Fully-specialised Serial ASCII server type.
pub(super) type SerialAsciiServerInner =
    ServerServices<CAsciiTransport, CServerApp, SERVER_SERIAL_QUEUE_DEPTH>;

// ── ID helpers ────────────────────────────────────────────────────────────────

#[inline(always)]
pub(super) fn server_id_tag(id: MbusServerId) -> u8 {
    (id >> 8) as u8
}

#[inline(always)]
pub(super) fn server_id_index(id: MbusServerId) -> usize {
    (id & 0xFF) as usize
}

#[inline(always)]
pub(super) fn encode_server_id(tag: u8, index: usize) -> MbusServerId {
    ((tag as u16) << 8) | (index as u16)
}

#[inline(always)]
pub(super) fn is_tcp_server_id(id: MbusServerId) -> bool {
    id != MBUS_INVALID_SERVER_ID && server_id_tag(id) == TAG_TCP_SERVER
}

#[inline(always)]
pub(super) fn is_serial_rtu_server_id(id: MbusServerId) -> bool {
    id != MBUS_INVALID_SERVER_ID && server_id_tag(id) == TAG_SERIAL_RTU_SERVER
}

#[inline(always)]
pub(super) fn is_serial_ascii_server_id(id: MbusServerId) -> bool {
    id != MBUS_INVALID_SERVER_ID && server_id_tag(id) == TAG_SERIAL_ASCII_SERVER
}

#[inline(always)]
pub(super) fn is_serial_server_id(id: MbusServerId) -> bool {
    is_serial_rtu_server_id(id) || is_serial_ascii_server_id(id)
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

// ── Pool struct ───────────────────────────────────────────────────────────────

struct ServerPool {
    tcp_slots: [Slot<TcpServerInner>; MAX_TCP_SERVERS],
    serial_rtu_slots: [Slot<SerialRtuServerInner>; MAX_SERIAL_SERVERS],
    serial_ascii_slots: [Slot<SerialAsciiServerInner>; MAX_SERIAL_SERVERS],
}

impl ServerPool {
    const fn new() -> Self {
        Self {
            tcp_slots: [const { Slot::empty() }; MAX_TCP_SERVERS],
            serial_rtu_slots: [const { Slot::empty() }; MAX_SERIAL_SERVERS],
            serial_ascii_slots: [const { Slot::empty() }; MAX_SERIAL_SERVERS],
        }
    }

    fn allocate_tcp(&mut self, value: TcpServerInner) -> Option<MbusServerId> {
        for (i, slot) in self.tcp_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_server_id(TAG_TCP_SERVER, i));
            }
        }
        None
    }

    fn allocate_serial_rtu(&mut self, value: SerialRtuServerInner) -> Option<MbusServerId> {
        for (i, slot) in self.serial_rtu_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_server_id(TAG_SERIAL_RTU_SERVER, i));
            }
        }
        None
    }

    fn allocate_serial_ascii(&mut self, value: SerialAsciiServerInner) -> Option<MbusServerId> {
        for (i, slot) in self.serial_ascii_slots.iter_mut().enumerate() {
            if !slot.occupied {
                slot.value = MaybeUninit::new(value);
                slot.borrow_flag.store(false, Ordering::SeqCst);
                slot.occupied = true;
                return Some(encode_server_id(TAG_SERIAL_ASCII_SERVER, i));
            }
        }
        None
    }

    fn free(&mut self, id: MbusServerId) -> bool {
        let idx = server_id_index(id);
        match server_id_tag(id) {
            TAG_TCP_SERVER => {
                if idx >= MAX_TCP_SERVERS {
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
            TAG_SERIAL_RTU_SERVER => {
                if idx >= MAX_SERIAL_SERVERS {
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
            TAG_SERIAL_ASCII_SERVER => {
                if idx >= MAX_SERIAL_SERVERS {
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

    fn is_occupied(&self, id: MbusServerId) -> bool {
        let idx = server_id_index(id);
        match server_id_tag(id) {
            TAG_TCP_SERVER => idx < MAX_TCP_SERVERS && self.tcp_slots[idx].occupied,
            TAG_SERIAL_RTU_SERVER => {
                idx < MAX_SERIAL_SERVERS && self.serial_rtu_slots[idx].occupied
            }
            TAG_SERIAL_ASCII_SERVER => {
                idx < MAX_SERIAL_SERVERS && self.serial_ascii_slots[idx].occupied
            }
            _ => false,
        }
    }
}

// ── Global static pool ────────────────────────────────────────────────────────

struct SyncServerPool(UnsafeCell<ServerPool>);
unsafe impl Sync for SyncServerPool {}

static SERVER_POOL: SyncServerPool = SyncServerPool(UnsafeCell::new(ServerPool::new()));

// ── Public pool operations ────────────────────────────────────────────────────

pub(super) fn server_pool_allocate_tcp(inner: TcpServerInner) -> Result<MbusServerId, MbusStatusCode> {
    let _guard = ServerPoolLockGuard::new();
    let pool = unsafe { &mut *SERVER_POOL.0.get() };
    pool.allocate_tcp(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

pub(super) fn server_pool_allocate_serial_rtu(
    inner: SerialRtuServerInner,
) -> Result<MbusServerId, MbusStatusCode> {
    let _guard = ServerPoolLockGuard::new();
    let pool = unsafe { &mut *SERVER_POOL.0.get() };
    pool.allocate_serial_rtu(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

pub(super) fn server_pool_allocate_serial_ascii(
    inner: SerialAsciiServerInner,
) -> Result<MbusServerId, MbusStatusCode> {
    let _guard = ServerPoolLockGuard::new();
    let pool = unsafe { &mut *SERVER_POOL.0.get() };
    pool.allocate_serial_ascii(inner)
        .ok_or(MbusStatusCode::MbusErrPoolFull)
}

pub(super) fn server_pool_free(id: MbusServerId) -> bool {
    let _server_guard = ServerLockGuard::new(id);
    let _pool_guard = ServerPoolLockGuard::new();
    let pool = unsafe { &mut *SERVER_POOL.0.get() };
    pool.free(id)
}

/// Borrow a TCP server and apply `f` to it.
pub(super) fn with_tcp_server<F, R>(id: MbusServerId, f: F) -> Result<R, MbusStatusCode>
where
    F: FnOnce(&mut TcpServerInner) -> R,
{
    if !is_tcp_server_id(id) {
        return Err(MbusStatusCode::MbusErrClientTypeMismatch);
    }

    let _guard = ServerLockGuard::new(id);
    let pool = unsafe { &mut *SERVER_POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    let idx = server_id_index(id);
    let slot = &mut pool.tcp_slots[idx];
    if slot.borrow_flag.swap(true, Ordering::SeqCst) {
        return Err(MbusStatusCode::MbusErrBusy);
    }
    let _borrow = ServerBorrowGuard::new(&slot.borrow_flag);

    let inner = unsafe { slot.value.assume_init_mut() };
    Ok(f(inner))
}

/// Internal serial dispatch helper.
macro_rules! dispatch_serial_server {
    ($id:expr, $pool:expr, $slots:ident, $f:expr) => {{
        let idx = server_id_index($id);
        let slot = &mut $pool.$slots[idx];
        if slot.borrow_flag.swap(true, Ordering::SeqCst) {
            return Err(MbusStatusCode::MbusErrBusy);
        }
        let _borrow = ServerBorrowGuard::new(&slot.borrow_flag);
        let inner = unsafe { slot.value.assume_init_mut() };
        Ok($f(inner))
    }};
}

/// Borrow a serial server (RTU or ASCII) and apply the matching closure.
pub(super) fn with_serial_server<F1, F2, R>(
    id: MbusServerId,
    f_rtu: F1,
    f_ascii: F2,
) -> Result<R, MbusStatusCode>
where
    F1: FnOnce(&mut SerialRtuServerInner) -> R,
    F2: FnOnce(&mut SerialAsciiServerInner) -> R,
{
    if !is_serial_server_id(id) {
        return Err(MbusStatusCode::MbusErrClientTypeMismatch);
    }

    let _guard = ServerLockGuard::new(id);
    let pool = unsafe { &mut *SERVER_POOL.0.get() };

    if !pool.is_occupied(id) {
        return Err(MbusStatusCode::MbusErrInvalidClientId);
    }

    if is_serial_rtu_server_id(id) {
        dispatch_serial_server!(id, pool, serial_rtu_slots, f_rtu)
    } else {
        dispatch_serial_server!(id, pool, serial_ascii_slots, f_ascii)
    }
}

/// Convenience macro to dispatch the same body to both serial variants.
macro_rules! with_serial_server_uniform {
    ($id:expr, |$inner:ident| $body:expr) => {
        $crate::c::server::pool::with_serial_server($id, |$inner| $body, |$inner| $body)
    };
}
pub(super) use with_serial_server_uniform;
