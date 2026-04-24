//! Discrete input service functions — ID-based C API.

use mbus_core::transport::UnitIdOrSlaveAddr;

use crate::c::error::MbusStatusCode;
use super::pool::{MbusClientId, with_serial_client_uniform, with_tcp_client};

/// Queue a Read Discrete Inputs (FC 0x02) request.
///
/// The response is delivered via [`MbusCallbacks::on_read_discrete_inputs`].
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_discrete_inputs(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    quantity: u16,
) -> MbusStatusCode {
    with_tcp_client(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.read_discrete_inputs(txn_id, uid, address, quantity) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read Discrete Inputs (FC 0x02) request on a serial client.
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_discrete_inputs(
    id: MbusClientId,
    txn_id: u16,
    unit_id: u8,
    address: u16,
    quantity: u16,
) -> MbusStatusCode {
    with_serial_client_uniform!(id, |inner| {
        let uid = match UnitIdOrSlaveAddr::new(unit_id) {
            Ok(u) => u,
            Err(e) => return MbusStatusCode::from(e),
        };
        match inner.read_discrete_inputs(txn_id, uid, address, quantity) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read Single Discrete Input request (FC 0x02 with quantity=1).
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_tcp_read_single_discrete_input(
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
        match inner.read_single_discrete_input(txn_id, uid, address) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}

/// Queue a Read Single Discrete Input request on a serial client.
#[cfg(feature = "discrete-inputs")]
#[unsafe(no_mangle)]
pub extern "C" fn mbus_serial_read_single_discrete_input(
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
        match inner.read_single_discrete_input(txn_id, uid, address) {
            Ok(()) => MbusStatusCode::MbusOk,
            Err(e) => MbusStatusCode::from(e),
        }
    })
    .unwrap_or_else(|e| e)
}
