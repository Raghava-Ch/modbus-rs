//! FIFO queue service functions — ID-based C API.

use mbus_core::transport::UnitIdOrSlaveAddr;

use super::error::MbusStatusCode;
use super::pool::{MbusClientId, pool_get_tcp, pool_get_serial};

/// Queue a Read FIFO Queue (FC 0x18) request.
///
/// The response is delivered via [`MbusCallbacks::on_read_fifo_queue`].
#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_fifo_queue(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    let inner = match pool_get_tcp(id) { Ok(c) => c, Err(e) => return e };
    let uid = match UnitIdOrSlaveAddr::new(unit_id) { Ok(u) => u, Err(e) => return MbusStatusCode::from(e) };
    match inner.read_fifo_queue(txn_id, uid, address) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}

/// Queue a Read FIFO Queue (FC 0x18) request on a serial client.
#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_fifo_queue(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
) -> MbusStatusCode {
    let inner = match pool_get_serial(id) { Ok(c) => c, Err(e) => return e };
    let uid = match UnitIdOrSlaveAddr::new(unit_id) { Ok(u) => u, Err(e) => return MbusStatusCode::from(e) };
    match inner.read_fifo_queue(txn_id, uid, address) {
        Ok(()) => MbusStatusCode::MbusOk,
        Err(e) => MbusStatusCode::from(e),
    }
}
