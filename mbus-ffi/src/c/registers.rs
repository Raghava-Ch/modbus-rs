//! Register service functions — ID-based C API.

use mbus_core::transport::UnitIdOrSlaveAddr;

use super::error::MbusStatusCode;
use super::pool::{MbusClientId, with_serial_client, with_tcp_client};

macro_rules! tcp_fn {
    ($name:ident, $method:ident $(, $arg:ident : $ty:ty)*) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            id: MbusClientId,
            txn_id: u16,
            unit_id: u8,
            $($arg: $ty,)*
        ) -> MbusStatusCode {
            with_tcp_client(id, |inner| {
                let uid = match UnitIdOrSlaveAddr::new(unit_id) {
                    Ok(u) => u,
                    Err(e) => return MbusStatusCode::from(e),
                };
                match inner.$method(txn_id, uid $(, $arg)*) {
                    Ok(()) => MbusStatusCode::MbusOk,
                    Err(e) => MbusStatusCode::from(e),
                }
            }).unwrap_or_else(|e| e)
        }
    };
}

macro_rules! serial_fn {
    ($name:ident, $method:ident $(, $arg:ident : $ty:ty)*) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            id: MbusClientId,
            txn_id: u16,
            unit_id: u8,
            $($arg: $ty,)*
        ) -> MbusStatusCode {
            with_serial_client(id, |inner| {
                let uid = match UnitIdOrSlaveAddr::new(unit_id) {
                    Ok(u) => u,
                    Err(e) => return MbusStatusCode::from(e),
                };
                match inner.$method(txn_id, uid $(, $arg)*) {
                    Ok(()) => MbusStatusCode::MbusOk,
                    Err(e) => MbusStatusCode::from(e),
                }
            }).unwrap_or_else(|e| e)
        }
    };
}

// ── Read holding registers (FC 0x03) ──────────────────────────────────────────

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_read_holding_registers,    read_holding_registers,        address: u16, quantity: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_read_holding_registers, read_holding_registers,     address: u16, quantity: u16);

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_read_single_holding_register,    read_single_holding_register,    address: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_read_single_holding_register, read_single_holding_register, address: u16);

// ── Read input registers (FC 0x04) ────────────────────────────────────────────

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_read_input_registers,    read_input_registers,        address: u16, quantity: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_read_input_registers, read_input_registers,     address: u16, quantity: u16);

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_read_single_input_register,    read_single_input_register,    address: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_read_single_input_register, read_single_input_register, address: u16);

// ── Write single register (FC 0x06) ───────────────────────────────────────────

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_write_single_register,    write_single_register,    address: u16, value: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_write_single_register, write_single_register, address: u16, value: u16);

// ── Write multiple registers (FC 0x10) ────────────────────────────────────────

/// Queue a Write Multiple Registers (FC 0x10) request.
///
/// `values` must point to at least `quantity` `u16` words, valid for this call only.
///
/// # Safety
/// `values` must be a valid non-null pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_write_multiple_registers(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    values: *const u16,
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
        let slice = unsafe { core::slice::from_raw_parts(values, quantity as usize) };
        match inner.write_multiple_registers(txn_id, uid, address, quantity, slice) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Write Multiple Registers (FC 0x10) request on a serial client.
///
/// # Safety
/// `values` must be a valid non-null pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_write_multiple_registers(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    values: *const u16,
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
        let slice = unsafe { core::slice::from_raw_parts(values, quantity as usize) };
        match inner.write_multiple_registers(txn_id, uid, address, quantity, slice) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

// ── Mask write register (FC 0x16) ─────────────────────────────────────────────

#[cfg(feature = "registers")]
tcp_fn!(mbus_tcp_mask_write_register,    mask_write_register,    address: u16, and_mask: u16, or_mask: u16);
#[cfg(feature = "registers")]
serial_fn!(mbus_serial_mask_write_register, mask_write_register, address: u16, and_mask: u16, or_mask: u16);

// ── Read/Write multiple registers (FC 0x17) ───────────────────────────────────

/// Queue a Read/Write Multiple Registers (FC 0x17) request.
///
/// `write_values` must point to at least `write_qty` valid `u16` words.
///
/// # Safety
/// `write_values` must be a valid non-null pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_read_write_multiple_registers(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    read_address: u16,
    read_qty: u16,
    write_address: u16,
    write_values: *const u16,
    write_qty: u16,
) -> MbusStatusCode {
    if write_values.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_tcp_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        let write_slice = unsafe { core::slice::from_raw_parts(write_values, write_qty as usize) };
        match inner.read_write_multiple_registers(
            txn_id,
            uid,
            read_address,
            read_qty,
            write_address,
            write_slice,
        ) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read/Write Multiple Registers (FC 0x17) request on a serial client.
///
/// # Safety
/// `write_values` must be a valid non-null pointer.
#[cfg(feature = "registers")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_read_write_multiple_registers(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    read_address: u16,
    read_qty: u16,
    write_address: u16,
    write_values: *const u16,
    write_qty: u16,
) -> MbusStatusCode {
    if write_values.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_serial_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        let write_slice = unsafe { core::slice::from_raw_parts(write_values, write_qty as usize) };
        match inner.read_write_multiple_registers(
            txn_id,
            uid,
            read_address,
            read_qty,
            write_address,
            write_slice,
        ) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}
