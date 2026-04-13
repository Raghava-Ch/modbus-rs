//! File record service functions — ID-based C API.

use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;
use mbus_core::models::file_record::SubRequest;
use mbus_core::transport::UnitIdOrSlaveAddr;

use super::error::MbusStatusCode;
use super::pool::{MbusClientId, with_serial_client_uniform, with_tcp_client};

/// A single sub-request passed to [`mbus_tcp_read_file_record`] /
/// [`mbus_tcp_write_file_record`] (and their serial equivalents).
///
/// For **read** operations: populate `file_number`, `record_number`, and
/// `data_len` (number of registers to read); set `data` to NULL.
///
/// For **write** operations: populate all four fields; `data` must point to
/// at least `data_len` valid `u16` words.
#[repr(C)]
pub struct MbusSubRequest {
    /// File number (1–65535).
    pub file_number: u16,
    /// Starting record number within the file.
    pub record_number: u16,
    /// Number of 16-bit registers (length). For reads: how many to read.
    /// For writes: must equal `data_len`.
    pub record_length: u16,
    /// Pointer to write data (NULL for reads). Valid only during the call.
    pub data: *const u16,
    /// Number of valid words pointed to by `data` (0 for reads).
    pub data_len: u16,
}

// ── Read file record ───────────────────────────────────────────────────────────

/// Queue a Read File Record (FC 0x14) request.
///
/// # Safety
/// `sub_reqs` must be a valid non-null pointer to `count` items.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_read_file_record(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode {
    if sub_reqs.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_tcp_client(id, |inner| {
        read_file_record_impl(inner, txn_id, unit_id, sub_reqs, count)
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read File Record (FC 0x14) request on a serial client.
///
/// # Safety
/// `sub_reqs` must be a valid non-null pointer to `count` items.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_read_file_record(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode {
    if sub_reqs.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_serial_client_uniform!(id, |inner| {
        read_file_record_impl(inner, txn_id, unit_id, sub_reqs, count)
    })
    .unwrap_or_else(|e| e)
}

#[cfg(feature = "file-record")]
fn read_file_record_impl<T, A, const N: usize>(
    inner: &mut mbus_client::services::ClientServices<T, A, N>,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode
where
    T: mbus_core::transport::Transport,
    A: mbus_client::services::ClientCommon + mbus_client::app::FileRecordResponse,
{
    let uid = match UnitIdOrSlaveAddr::new(unit_id) {
        Ok(u) => u,
        Err(e) => return MbusStatusCode::from(e),
    };
    let c_slice = unsafe { core::slice::from_raw_parts(sub_reqs, count as usize) };
    let mut sub_request = SubRequest::new();
    for sr in c_slice {
        if let Err(e) =
            sub_request.add_read_sub_request(sr.file_number, sr.record_number, sr.record_length)
        {
            return MbusStatusCode::from(e);
        }
    }
    match inner.read_file_record(txn_id, uid, &sub_request) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}

// ── Write file record ─────────────────────────────────────────────────────────

/// Queue a Write File Record (FC 0x15) request.
///
/// # Safety
/// `sub_reqs` must be valid. Each `sub_reqs[i].data` must be valid for
/// `sub_reqs[i].data_len` words.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_tcp_write_file_record(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode {
    if sub_reqs.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_tcp_client(id, |inner| {
        write_file_record_impl(inner, txn_id, unit_id, sub_reqs, count)
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Write File Record (FC 0x15) request on a serial client.
///
/// # Safety
/// `sub_reqs` must be valid. Each `sub_reqs[i].data` must be valid for
/// `sub_reqs[i].data_len` words.
#[cfg(feature = "file-record")]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbus_serial_write_file_record(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode {
    if sub_reqs.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }
    with_serial_client_uniform!(id, |inner| {
        write_file_record_impl(inner, txn_id, unit_id, sub_reqs, count)
    })
    .unwrap_or_else(|e| e)
}

#[cfg(feature = "file-record")]
fn write_file_record_impl<T, A, const N: usize>(
    inner: &mut mbus_client::services::ClientServices<T, A, N>,
    txn_id: u16,
    unit_id: u8,
    sub_reqs: *const MbusSubRequest,
    count: u16,
) -> MbusStatusCode
where
    T: mbus_core::transport::Transport,
    A: mbus_client::services::ClientCommon + mbus_client::app::FileRecordResponse,
{
    let uid = match UnitIdOrSlaveAddr::new(unit_id) {
        Ok(u) => u,
        Err(e) => return MbusStatusCode::from(e),
    };
    let c_slice = unsafe { core::slice::from_raw_parts(sub_reqs, count as usize) };
    let mut sub_request = SubRequest::new();
    for sr in c_slice {
        if sr.data.is_null() || sr.data_len == 0 {
            return MbusStatusCode::MbusErrNullPointer;
        }
        let word_slice = unsafe { core::slice::from_raw_parts(sr.data, sr.data_len as usize) };
        let mut hvec: HVec<u16, MAX_PDU_DATA_LEN> = HVec::new();
        if hvec.extend_from_slice(word_slice).is_err() {
            return MbusStatusCode::MbusErrBufferTooSmall;
        }
        if let Err(e) = sub_request.add_write_sub_request(
            sr.file_number,
            sr.record_number,
            sr.record_length,
            hvec,
        ) {
            return MbusStatusCode::from(e);
        }
    }
    match inner.write_file_record(txn_id, uid, &sub_request) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}
