//! Diagnostics service functions — ID-based C API.

use mbus_core::function_codes::public::DiagnosticSubFunction;
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
use mbus_core::transport::UnitIdOrSlaveAddr;

use super::error::MbusStatusCode;
use super::pool::{MbusClientId, pool_get_tcp, pool_get_serial};

macro_rules! tcp_diag_fn {
    ($name:ident, $method:ident $(, $arg:ident : $ty:ty)*) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            id: MbusClientId,
            txn_id: u16,
            unit_id: u8,
            $($arg: $ty,)*
        ) -> MbusStatusCode {
            let inner = match pool_get_tcp(id) { Ok(c) => c, Err(e) => return e };
            let uid = match UnitIdOrSlaveAddr::new(unit_id) { Ok(u) => u, Err(e) => return MbusStatusCode::from(e) };
            match inner.$method(txn_id, uid $(, $arg)*) {
                Ok(()) => MbusStatusCode::MbusOk,
                Err(e) => MbusStatusCode::from(e),
            }
        }
    };
}

macro_rules! serial_diag_fn {
    ($name:ident, $method:ident $(, $arg:ident : $ty:ty)*) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            id: MbusClientId,
            txn_id: u16,
            unit_id: u8,
            $($arg: $ty,)*
        ) -> MbusStatusCode {
            let inner = match pool_get_serial(id) { Ok(c) => c, Err(e) => return e };
            let uid = match UnitIdOrSlaveAddr::new(unit_id) { Ok(u) => u, Err(e) => return MbusStatusCode::from(e) };
            match inner.$method(txn_id, uid $(, $arg)*) {
                Ok(()) => MbusStatusCode::MbusOk,
                Err(e) => MbusStatusCode::from(e),
            }
        }
    };
}

// ── Read Exception Status (FC 0x07) ───────────────────────────────────────────

#[cfg(feature = "diagnostics")]
tcp_diag_fn!(mbus_tcp_read_exception_status, read_exception_status);
#[cfg(feature = "diagnostics")]
serial_diag_fn!(mbus_serial_read_exception_status, read_exception_status);

// ── Get Comm Event Counter (FC 0x0B) ──────────────────────────────────────────

#[cfg(feature = "diagnostics")]
tcp_diag_fn!(mbus_tcp_get_comm_event_counter, get_comm_event_counter);
#[cfg(feature = "diagnostics")]
serial_diag_fn!(mbus_serial_get_comm_event_counter, get_comm_event_counter);

// ── Get Comm Event Log (FC 0x0C) ──────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
tcp_diag_fn!(mbus_tcp_get_comm_event_log, get_comm_event_log);
#[cfg(feature = "diagnostics")]
serial_diag_fn!(mbus_serial_get_comm_event_log, get_comm_event_log);

// ── Report Server ID (FC 0x11) ────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
tcp_diag_fn!(mbus_tcp_report_server_id, report_server_id);
#[cfg(feature = "diagnostics")]
serial_diag_fn!(mbus_serial_report_server_id, report_server_id);

// ── Diagnostics (FC 0x08) ─────────────────────────────────────────────────────

/// Queue a Diagnostics (FC 0x08) request.
///
/// # Safety
/// If `data_len > 0`, `data` must be valid for that many `u16` words.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_diagnostics(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_fn: u16,
    data: *const u16,
    data_len: u16,
) -> MbusStatusCode {
    let inner = match pool_get_tcp(id) { Ok(c) => c, Err(e) => return e };
    diagnostics_impl(inner, txn_id, unit_id, sub_fn, data, data_len)
}

/// Queue a Diagnostics (FC 0x08) request on a serial client.
///
/// # Safety
/// If `data_len > 0`, `data` must be valid for that many `u16` words.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_diagnostics(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_fn: u16,
    data: *const u16,
    data_len: u16,
) -> MbusStatusCode {
    let inner = match pool_get_serial(id) { Ok(c) => c, Err(e) => return e };
    diagnostics_impl(inner, txn_id, unit_id, sub_fn, data, data_len)
}

#[cfg(feature = "diagnostics")]
fn diagnostics_impl<T, A, const N: usize>(
    inner: &mut mbus_client::services::ClientServices<T, A, N>,
    txn_id: u16,
    unit_id: u8,
    sub_fn: u16,
    data: *const u16,
    data_len: u16,
) -> MbusStatusCode
where
    T: mbus_core::transport::Transport,
    A: mbus_client::services::ClientCommon + mbus_client::app::DiagnosticsResponse,
{
    let uid = match UnitIdOrSlaveAddr::new(unit_id) {
        Ok(u) => u,
        Err(e) => return MbusStatusCode::from(e),
    };
    let sub_function = match DiagnosticSubFunction::try_from(sub_fn) {
        Ok(f) => f,
        Err(e) => return MbusStatusCode::from(e),
    };
    let slice = if data_len > 0 && !data.is_null() {
        unsafe { core::slice::from_raw_parts(data, data_len as usize) }
    } else {
        &[]
    };
    match inner.diagnostics(txn_id, uid, sub_function, slice) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}

// ── Read Device Identification (FC 0x2B / MEI 0x0E) ──────────────────────────

/// Queue a Read Device Identification (FC 0x2B/0x0E) request.
///
/// `dev_id_code`: 1=Basic, 2=Regular, 3=Extended, 4=Specific.
/// `object_id`: starting object ID (0x00–0xFF).
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_device_identification(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    dev_id_code: u8,
    object_id: u8,
) -> MbusStatusCode {
    let inner = match pool_get_tcp(id) { Ok(c) => c, Err(e) => return e };
    read_device_id_impl(inner, txn_id, unit_id, dev_id_code, object_id)
}

/// Queue a Read Device Identification request on a serial client.
#[cfg(feature = "diagnostics")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_device_identification(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    dev_id_code: u8,
    object_id: u8,
) -> MbusStatusCode {
    let inner = match pool_get_serial(id) { Ok(c) => c, Err(e) => return e };
    read_device_id_impl(inner, txn_id, unit_id, dev_id_code, object_id)
}

#[cfg(feature = "diagnostics")]
fn read_device_id_impl<T, A, const N: usize>(
    inner: &mut mbus_client::services::ClientServices<T, A, N>,
    txn_id: u16,
    unit_id: u8,
    dev_id_code: u8,
    object_id: u8,
) -> MbusStatusCode
where
    T: mbus_core::transport::Transport,
    A: mbus_client::services::ClientCommon + mbus_client::app::DiagnosticsResponse,
{
    let uid = match UnitIdOrSlaveAddr::new(unit_id) {
        Ok(u) => u,
        Err(e) => return MbusStatusCode::from(e),
    };
    let code = match ReadDeviceIdCode::try_from(dev_id_code) {
        Ok(c) => c,
        Err(e) => return MbusStatusCode::from(e),
    };
    let oid = ObjectId::from(object_id);
    match inner.read_device_identification(txn_id, uid, code, oid) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}
