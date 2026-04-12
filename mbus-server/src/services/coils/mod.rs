//! # Modbus Coil Service (server-side)
//!
//! Handles coil-oriented Modbus requests (FC01/FC05/FC15) and builds
//! response PDUs.

use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use crate::app::ModbusAppHandler;
use crate::services::framing::{
    build_byte_count_prefixed_response, build_echo_u16_response, parse_read_window,
    parse_write_multiple_request, parse_write_single_request,
};
use crate::services::{ServerServices, server_log_debug};

/// FC01 quantity lower bound (inclusive).
const FC01_MIN_QUANTITY: u16 = 1;
/// FC01 quantity upper bound (inclusive).
const FC01_MAX_QUANTITY: u16 = 2000;
/// FC15 quantity lower bound (inclusive).
const FC15_MIN_QUANTITY: u16 = 1;
/// FC15 quantity upper bound (inclusive).
const FC15_MAX_QUANTITY: u16 = 1968;

impl<TRANSPORT, APP> ServerServices<TRANSPORT, APP>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC01 (Read Coils).
    ///
    /// Validates the read window and quantity bounds, requests packed coil
    /// bits from the application callback, and sends a byte-count-prefixed
    /// response frame.
    pub(super) fn handle_read_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let (address, quantity) = match parse_read_window(message) {
            Ok(values) => values,
            Err(err) => {
                server_log_debug!("FC01: failed to parse request: {:?}", err);
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadCoils,
                    err,
                );
                return;
            }
        };

        if !(FC01_MIN_QUANTITY..=FC01_MAX_QUANTITY).contains(&quantity) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadCoils,
                MbusError::InvalidQuantity,
            );
            return;
        }

        if address.checked_add(quantity - 1).is_none() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadCoils,
                MbusError::InvalidAddress,
            );
            return;
        }

        let expected_len = packed_bit_len(quantity);
        let mut buf = [0u8; MAX_PDU_DATA_LEN];
        let length = match self.app.read_coils_request(
            txn_id,
            unit_id_or_slave_addr,
            address,
            quantity,
            &mut buf,
        ) {
            Ok(length) => length,
            Err(err) => {
                server_log_debug!(
                    "FC01: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadCoils,
                    err,
                );
                return;
            }
        };

        if length as usize > buf.len() || length != expected_len as u8 {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::ReadCoils,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let response = match build_byte_count_prefixed_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::ReadCoils,
            &buf[..length as usize],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::ReadCoils,
                    err,
                );
                return;
            }
        };

        if let Err(err) = self.transport.send(&response) {
            server_log_debug!("FC01: transport send failed: {:?}", err);
        }
    }

    /// Handles FC05 (Write Single Coil).
    ///
    /// Parses the address/value pair, validates Modbus coil semantics
    /// (`0xFF00` for ON, `0x0000` for OFF), dispatches the write callback,
    /// and responds with the standard write echo.
    pub(super) fn handle_write_single_coil_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let (address, raw_value) = match parse_write_single_request(message) {
            Ok(values) => values,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteSingleCoil,
                    err,
                );
                return;
            }
        };

        let value = match raw_value {
            0xFF00 => true,
            0x0000 => false,
            _ => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteSingleCoil,
                    MbusError::InvalidValue,
                );
                return;
            }
        };

        if let Err(err) =
            self.app
                .write_single_coil_request(txn_id, unit_id_or_slave_addr, address, value)
        {
            server_log_debug!(
                "FC05: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteSingleCoil,
                err,
            );
            return;
        }

        // Per Modbus spec, the FC05 echo must mirror the raw request value (0xFF00
        // or 0x0000) — not the decoded bool.  The app callback intentionally
        // receives a bool; the echo intentionally uses raw_value.
        let response = match build_echo_u16_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::WriteSingleCoil,
            address,
            raw_value,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteSingleCoil,
                    err,
                );
                return;
            }
        };

        if let Err(err) = self.transport.send(&response) {
            server_log_debug!("FC05: transport send failed: {:?}", err);
        }
    }

    /// Handles FC15 (Write Multiple Coils).
    ///
    /// Validates quantity bounds, address overflow, and packed-byte layout,
    /// writes the requested coil range through the application callback, then
    /// returns the Modbus echo of start address and quantity.
    pub(super) fn handle_write_multiple_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let (address, quantity, byte_count, values) = match parse_write_multiple_request(message) {
            Ok(values) => values,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteMultipleCoils,
                    err,
                );
                return;
            }
        };

        if !(FC15_MIN_QUANTITY..=FC15_MAX_QUANTITY).contains(&quantity) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleCoils,
                MbusError::InvalidQuantity,
            );
            return;
        }
        if address.checked_add(quantity - 1).is_none() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleCoils,
                MbusError::InvalidAddress,
            );
            return;
        }

        let expected_byte_count = packed_bit_len(quantity);
        if byte_count as usize != expected_byte_count || values.len() != expected_byte_count {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleCoils,
                MbusError::InvalidByteCount,
            );
            return;
        }

        if let Err(err) = self.app.write_multiple_coils_request(
            txn_id,
            unit_id_or_slave_addr,
            address,
            quantity,
            values,
        ) {
            server_log_debug!(
                "FC0F: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleCoils,
                err,
            );
            return;
        }

        let response = match build_echo_u16_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::WriteMultipleCoils,
            address,
            quantity,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteMultipleCoils,
                    err,
                );
                return;
            }
        };

        if let Err(err) = self.transport.send(&response) {
            server_log_debug!("FC15: transport send failed: {:?}", err);
        }
    }
}

/// Returns the number of packed bytes needed to represent a coil quantity.
fn packed_bit_len(quantity: u16) -> usize {
    (quantity as usize).div_ceil(8)
}
