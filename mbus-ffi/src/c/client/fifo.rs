//! FIFO queue service functions — ID-based C API.

use mbus_core::transport::UnitIdOrSlaveAddr;

use super::pool::MbusClientId;

#[cfg(feature = "network-tcp")]
use super::pool::with_tcp_client;

#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
use super::pool::with_serial_client_uniform;
use crate::c::error::MbusStatusCode;

/// Queue a Read FIFO Queue (FC 0x18) request.
///
/// The response is delivered via `MbusCallbacks::on_read_fifo_queue`.
#[cfg(all(feature = "fifo", feature = "network-tcp"))]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_fifo_queue(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    with_tcp_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.read_fifo_queue(txn_id, uid, address) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read FIFO Queue (FC 0x18) request on a serial client.
#[cfg(all(
    feature = "fifo",
    any(feature = "serial-rtu", feature = "serial-ascii")
))]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_fifo_queue(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    with_serial_client_uniform!(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.read_fifo_queue(txn_id, uid, address) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}
