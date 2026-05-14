//! Stub implementations for extern "C" locking functions.
//!
//! When building as a cdylib (e.g., for NAPI, Python, Go, .NET),
//! the locking functions are not provided by a C caller.
//! This module provides no-op stub implementations to satisfy the linker.
//!
//! For C FFI builds (staticlib/cdylib with C caller), the actual
//! implementations are expected to be provided by the C application.

/// No-op stub for pool lock (used when no external C caller provides it).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_pool_lock() {}

/// No-op stub for pool unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_pool_unlock() {}

/// No-op stub for client lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_client_lock(_id: u16) {}

/// No-op stub for client unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_client_unlock(_id: u16) {}

/// No-op stub for server lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_lock(_id: u16) {}

/// No-op stub for server unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_unlock(_id: u16) {}

/// No-op stub for gateway lock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_lock(_id: u8) {}

/// No-op stub for gateway unlock.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_unlock(_id: u8) {}
