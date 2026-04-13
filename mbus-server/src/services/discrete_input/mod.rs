//! # Modbus Discrete Input Service (server-side)
//!
//! Handles FC02 (Read Discrete Inputs) requests and builds response PDUs.

use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use crate::app::ModbusAppHandler;
use crate::services::framing::{build_byte_count_prefixed_response, parse_read_window};
use crate::services::{ServerServices, server_log_debug};

/// FC02 quantity lower bound (inclusive).
const FC02_MIN_QUANTITY: u16 = 1;
/// FC02 quantity upper bound (inclusive).
const FC02_MAX_QUANTITY: u16 = 2000;

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC02 (Read Discrete Inputs).
    ///
    /// Validates the read window and quantity bounds, requests packed input bits
    /// from the application callback, and sends a byte-count-prefixed response.
    pub(super) fn handle_read_discrete_inputs_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let (address, quantity) = match parse_read_window(message) {
            Ok(values) => values,
            Err(err) => {
                server_log_debug!("FC02: failed to parse request: {:?}", err);
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadDiscreteInputs,
                    err,
                );
                return;
            }
        };

        if !(FC02_MIN_QUANTITY..=FC02_MAX_QUANTITY).contains(&quantity) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadDiscreteInputs,
                MbusError::InvalidQuantity,
            );
            return;
        }

        if address.checked_add(quantity - 1).is_none() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadDiscreteInputs,
                MbusError::InvalidAddress,
            );
            return;
        }

        let expected_len = packed_bit_len(quantity);
        let mut buf = [0u8; MAX_PDU_DATA_LEN];
        let length = match self.app.read_discrete_inputs_request(
            txn_id,
            unit_id_or_slave_addr,
            address,
            quantity,
            &mut buf,
        ) {
            Ok(length) => length,
            Err(err) => {
                server_log_debug!(
                    "FC02: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadDiscreteInputs,
                    err,
                );
                return;
            }
        };

        if length as usize > buf.len() || length != expected_len as u8 {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadDiscreteInputs,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let response = match build_byte_count_prefixed_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::ReadDiscreteInputs,
            &buf[..length as usize],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadDiscreteInputs,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }
}

#[inline]
fn packed_bit_len(quantity: u16) -> usize {
    quantity.div_ceil(8) as usize
}
