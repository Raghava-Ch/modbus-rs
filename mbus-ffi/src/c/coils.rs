//! Coil service functions — ID-based C API.

#[cfg(feature = "coils")]
use mbus_client::services::coil::Coils;
use mbus_core::transport::UnitIdOrSlaveAddr;

use super::error::MbusStatusCode;
use super::pool::{MbusClientId, with_serial_client, with_tcp_client};

macro_rules! call_tcp {
    ($id:ident, $method:ident, $txn_id:ident, $unit_id:ident $(, $arg:ident)*) => {{
        with_tcp_client($id, |inner| {
            let uid = match UnitIdOrSlaveAddr::new($unit_id) {
                Ok(u) => u,
                Err(e) => return MbusStatusCode::from(e),
            };
            match inner.$method($txn_id, uid $(, $arg)*) {
                Ok(()) => MbusStatusCode::MbusOk,
                Err(e) => MbusStatusCode::from(e),
            }
        }).unwrap_or_else(|e| e)
    }};
}

macro_rules! call_serial {
    ($id:ident, $method:ident, $txn_id:ident, $unit_id:ident $(, $arg:ident)*) => {{
        with_serial_client($id, |inner| {
            let uid = match UnitIdOrSlaveAddr::new($unit_id) {
                Ok(u) => u,
                Err(e) => return MbusStatusCode::from(e),
            };
            match inner.$method($txn_id, uid $(, $arg)*) {
                Ok(()) => MbusStatusCode::MbusOk,
                Err(e) => MbusStatusCode::from(e),
            }
        }).unwrap_or_else(|e| e)
    }};
}

// ── Read coils ────────────────────────────────────────────────────────────────

/// Queue a Read Coils (FC 0x01) request.
///
/// The response is delivered via [`MbusCallbacks::on_read_coils`].
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_coils(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    quantity: u16,
) -> MbusStatusCode {
    call_tcp!(id, read_multiple_coils, txn_id, unit_id, address, quantity)
}

/// Queue a Read Coils (FC 0x01) request on a serial client.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_coils(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    quantity: u16,
) -> MbusStatusCode {
    call_serial!(id, read_multiple_coils, txn_id, unit_id, address, quantity)
}

// ── Read single coil ──────────────────────────────────────────────────────────

/// Queue a Read Single Coil request (reads FC 0x01 with quantity=1).
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_single_coil(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    call_tcp!(id, read_single_coil, txn_id, unit_id, address)
}

/// Queue a Read Single Coil request on a serial client.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_single_coil(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    call_serial!(id, read_single_coil, txn_id, unit_id, address)
}

// ── Write single coil ─────────────────────────────────────────────────────────

/// Queue a Write Single Coil (FC 0x05) request. `value`: 1 = ON, 0 = OFF.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_write_single_coil(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    value: u8,
) -> MbusStatusCode {
    with_tcp_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.write_single_coil(txn_id, uid, address, value != 0) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Write Single Coil (FC 0x05) request on a serial client.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_write_single_coil(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    value: u8,
) -> MbusStatusCode {
    with_serial_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.write_single_coil(txn_id, uid, address, value != 0) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

// ── Write multiple coils ──────────────────────────────────────────────────────

/// Queue a Write Multiple Coils (FC 0x0F) request.
///
/// `values` must point to at least `ceil(quantity / 8)` bytes of bit-packed coil
/// data (LSB-first). Valid for the duration of this call only.
///
/// # Safety
/// `values` must be a valid non-null pointer.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_write_multiple_coils(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    values: *const u8,
    quantity: u16,
) -> MbusStatusCode {
    if values.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_tcp_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        let byte_count = ((quantity + 7) / 8) as usize;
        let value_slice = unsafe { core::slice::from_raw_parts(values, byte_count) };

        let coils = match Coils::new(address, quantity) {
            Ok(c) => c,
            Err(e) => return MbusStatusCode::from(e),
        };
        let coils = match coils.with_values(value_slice, quantity) {
            Ok(c) => c,
            Err(e) => return MbusStatusCode::from(e),
        };

        match inner.write_multiple_coils(txn_id, uid, address, &coils) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Write Multiple Coils (FC 0x0F) request on a serial client.
///
/// # Safety
/// `values` must be a valid non-null pointer.
#[cfg(feature = "coils")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_write_multiple_coils(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    values: *const u8,
    quantity: u16,
) -> MbusStatusCode {
    if values.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_serial_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        let byte_count = ((quantity + 7) / 8) as usize;
        let value_slice = unsafe { core::slice::from_raw_parts(values, byte_count) };

        let coils = match Coils::new(address, quantity) {
            Ok(c) => c,
            Err(e) => return MbusStatusCode::from(e),
        };
        let coils = match coils.with_values(value_slice, quantity) {
            Ok(c) => c,
            Err(e) => return MbusStatusCode::from(e),
        };

        match inner.write_multiple_coils(txn_id, uid, address, &coils) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}
