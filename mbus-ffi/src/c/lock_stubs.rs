//! Stub implementations for extern "C" locking functions.
//!
//! When building as a cdylib (e.g., for NAPI, Python, Go, .NET),
//! the locking functions are not provided by a C caller.
//! This module provides no-op stub implementations to satisfy the linker.
//!
//! For C FFI builds (staticlib/cdylib with C caller), the actual
//! implementations are expected to be provided by the C application.

/// No-op stub for pool lock (used when no external C caller provides it).
#[cfg(any(feature = "c-client", feature = "c-server", feature = "c-gateway"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_pool_lock() {}

/// No-op stub for pool unlock.
#[cfg(any(feature = "c-client", feature = "c-server", feature = "c-gateway"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_pool_unlock() {}

/// No-op stub for client lock.
#[cfg(feature = "c-client")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_client_lock(_id: u16) {}

/// No-op stub for client unlock.
#[cfg(feature = "c-client")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_client_unlock(_id: u16) {}

/// No-op stub for server lock.
#[cfg(feature = "c-server")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_lock(_id: u16) {}

/// No-op stub for server unlock.
#[cfg(feature = "c-server")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_server_unlock(_id: u16) {}

/// No-op stub for gateway lock.
#[cfg(feature = "c-gateway")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_lock(_id: u8) {}

/// No-op stub for gateway unlock.
#[cfg(feature = "c-gateway")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_gateway_unlock(_id: u8) {}
