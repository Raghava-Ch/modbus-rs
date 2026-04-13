//! # Modbus Register Service (server-side)
//!
//! Handles register-oriented Modbus requests (FC03/FC04/FC06/FC10) and
//! builds the corresponding response PDUs.

use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

use super::framing::{build_byte_count_prefixed_response, parse_read_window};
#[cfg(feature = "holding-registers")]
use super::framing::{
    build_echo_u16_response, parse_write_multiple_request, parse_write_single_request,
};
use crate::app::ModbusAppHandler;
use crate::services::{ServerServices, server_log_debug, server_log_trace};

/// FC03 quantity lower bound (inclusive).
#[cfg(feature = "holding-registers")]
const FC03_MIN_QUANTITY: u16 = 1;
/// FC03 quantity upper bound (inclusive).
#[cfg(feature = "holding-registers")]
const FC03_MAX_QUANTITY: u16 = 125;
/// FC04 quantity lower bound (inclusive).
#[cfg(feature = "input-registers")]
const FC04_MIN_QUANTITY: u16 = 1;
/// FC04 quantity upper bound (inclusive).
#[cfg(feature = "input-registers")]
const FC04_MAX_QUANTITY: u16 = 125;
/// FC16 register count lower bound (inclusive).
#[cfg(feature = "holding-registers")]
const FC16_MIN_QUANTITY: u16 = 1;
/// FC16 register count upper bound (inclusive).
#[cfg(feature = "holding-registers")]
const FC16_MAX_QUANTITY: u16 = 123;

impl<TRANSPORT, APP, const QUEUE_DEPTH: usize> ServerServices<TRANSPORT, APP, QUEUE_DEPTH>
where
    TRANSPORT: Transport,
    APP: ModbusAppHandler,
{
    /// Handles FC03 (Read Holding Registers).
    ///
    /// Validates the request window and quantity bounds, invokes the
    /// application callback, and sends a byte-count-prefixed register response.
    #[cfg(feature = "holding-registers")]
    pub(super) fn handle_read_holding_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        self.handle_register_read(
            txn_id,
            unit_id_or_slave_addr,
            message,
            FunctionCode::ReadHoldingRegisters,
            FC03_MIN_QUANTITY..=FC03_MAX_QUANTITY,
            |app, address, quantity, out| {
                app.read_multiple_holding_registers_request(
                    txn_id,
                    unit_id_or_slave_addr,
                    address,
                    quantity,
                    out,
                )
            },
        );
    }

    /// Handles FC04 (Read Input Registers).
    ///
    /// Uses the shared register-read pipeline to validate the request,
    /// execute the app callback, and encode the response frame.
    #[cfg(feature = "input-registers")]
    pub(super) fn handle_read_input_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        self.handle_register_read(
            txn_id,
            unit_id_or_slave_addr,
            message,
            FunctionCode::ReadInputRegisters,
            FC04_MIN_QUANTITY..=FC04_MAX_QUANTITY,
            |app, address, quantity, out| {
                app.read_multiple_input_registers_request(
                    txn_id,
                    unit_id_or_slave_addr,
                    address,
                    quantity,
                    out,
                )
            },
        );
    }

    /// Handles FC06 (Write Single Register).
    ///
    /// Parses the address/value pair, calls the app write callback, and sends
    /// an echo response with the same address and value.
    #[cfg(feature = "holding-registers")]
    pub(super) fn handle_write_single_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
    ) {
        let (address, value) = match parse_write_single_request(message) {
            Ok(values) => values,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteSingleRegister,
                    err,
                );
                return;
            }
        };

        if let Err(err) =
            self.app
                .write_single_register_request(txn_id, unit_id_or_slave_addr, address, value)
        {
            server_log_debug!(
                "FC06: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteSingleRegister,
                err,
            );
            return;
        }

        let response = match build_echo_u16_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::WriteSingleRegister,
            address,
            value,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteSingleRegister,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles a Serial broadcast FC06 request without emitting any response.
    pub(super) fn handle_broadcast_write_single_register_request(
        &mut self,
        message: &ModbusMessage,
    ) {
        let txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let (address, value) = match parse_write_single_request(message) {
            Ok(values) => values,
            Err(err) => {
                server_log_debug!(
                    "FC06 broadcast ignored due to invalid request: txn_id={}, error={:?}",
                    txn_id,
                    err
                );
                return;
            }
        };

        if let Err(err) =
            self.app
                .write_single_register_request(txn_id, unit_id_or_slave_addr, address, value)
        {
            server_log_debug!(
                "FC06 broadcast app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
        }
    }

    /// Handles FC16 (Write Multiple Registers).
    ///
    /// Validates quantity and byte-count consistency, converts payload bytes
    /// to big-endian register values, calls the app callback, and returns the
    /// standard echo response with start address and quantity.
    #[cfg(feature = "holding-registers")]
    pub(super) fn handle_write_multiple_registers_request(
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
                    FunctionCode::WriteMultipleRegisters,
                    err,
                );
                return;
            }
        };

        if !(FC16_MIN_QUANTITY..=FC16_MAX_QUANTITY).contains(&quantity) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleRegisters,
                MbusError::InvalidQuantity,
            );
            return;
        }
        if address.checked_add(quantity - 1).is_none() {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleRegisters,
                MbusError::InvalidAddress,
            );
            return;
        }

        let expected_byte_count = quantity as usize * 2;
        if byte_count as usize != expected_byte_count || values.len() != expected_byte_count {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleRegisters,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let mut registers = [0u16; FC16_MAX_QUANTITY as usize];
        for (index, chunk) in values.chunks_exact(2).enumerate() {
            registers[index] = u16::from_be_bytes([chunk[0], chunk[1]]);
        }

        if let Err(err) = self.app.write_multiple_registers_request(
            txn_id,
            unit_id_or_slave_addr,
            address,
            &registers[..quantity as usize],
        ) {
            server_log_debug!(
                "FC10: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                FunctionCode::WriteMultipleRegisters,
                err,
            );
            return;
        }

        let response = match build_echo_u16_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            FunctionCode::WriteMultipleRegisters,
            address,
            quantity,
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(
                    txn_id,
                    unit_id_or_slave_addr,
                    FunctionCode::WriteMultipleRegisters,
                    err,
                );
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Shared implementation for FC03/FC04-style register reads.
    ///
    /// Performs request parsing, quantity/address validation, callback
    /// invocation, payload length checks, and response encoding/sending.
    fn handle_register_read<F>(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        message: &ModbusMessage,
        function_code: FunctionCode,
        quantity_range: core::ops::RangeInclusive<u16>,
        handler: F,
    ) where
        F: FnOnce(&mut APP, u16, u16, &mut [u8]) -> Result<u8, MbusError>,
    {
        let (address, quantity) = match parse_read_window(message) {
            Ok(values) => values,
            Err(err) => {
                server_log_debug!(
                    "FC{:02X}: failed to parse request: {:?}",
                    function_code as u8,
                    err
                );
                self.send_exception_response(txn_id, unit_id_or_slave_addr, function_code, err);
                return;
            }
        };

        if !quantity_range.contains(&quantity) {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                function_code,
                MbusError::InvalidQuantity,
            );
            return;
        }

        let Some(end_addr) = address.checked_add(quantity - 1) else {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                function_code,
                MbusError::InvalidAddress,
            );
            return;
        };
        server_log_trace!(
            "FC{:02X}: validated request range start={}, end={}, quantity={}",
            function_code as u8,
            address,
            end_addr,
            quantity
        );

        let mut buf = [0u8; MAX_PDU_DATA_LEN];
        let length = match handler(&mut self.app, address, quantity, &mut buf) {
            Ok(n) => n,
            Err(err) => {
                server_log_debug!(
                    "FC{:02X}: app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                    function_code as u8,
                    txn_id,
                    unit_id_or_slave_addr.get(),
                    err
                );
                self.send_exception_response(txn_id, unit_id_or_slave_addr, function_code, err);
                return;
            }
        };

        let expected_len = quantity as usize * 2;
        if length as usize > buf.len() || length as usize != expected_len {
            self.send_exception_response(
                txn_id,
                unit_id_or_slave_addr,
                function_code,
                MbusError::InvalidByteCount,
            );
            return;
        }

        let response = match build_byte_count_prefixed_response(
            &self.transport,
            txn_id,
            unit_id_or_slave_addr,
            function_code,
            &buf[..length as usize],
        ) {
            Ok(frame) => frame,
            Err(err) => {
                self.send_exception_response(txn_id, unit_id_or_slave_addr, function_code, err);
                return;
            }
        };

        self.try_send_or_queue(&response, txn_id, unit_id_or_slave_addr);
    }

    /// Handles a Serial broadcast FC10 request without emitting any response.
    pub(super) fn handle_broadcast_write_multiple_registers_request(
        &mut self,
        message: &ModbusMessage,
    ) {
        let txn_id = message.transaction_id();
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let (address, quantity, byte_count, values) = match parse_write_multiple_request(message) {
            Ok(values) => values,
            Err(err) => {
                server_log_debug!(
                    "FC10 broadcast ignored due to invalid request: txn_id={}, error={:?}",
                    txn_id,
                    err
                );
                return;
            }
        };

        if !(FC16_MIN_QUANTITY..=FC16_MAX_QUANTITY).contains(&quantity) {
            server_log_debug!(
                "FC10 broadcast ignored due to invalid quantity: txn_id={}, quantity={}",
                txn_id,
                quantity
            );
            return;
        }
        if address.checked_add(quantity - 1).is_none() {
            server_log_debug!(
                "FC10 broadcast ignored due to address overflow: txn_id={}, address={}, quantity={}",
                txn_id,
                address,
                quantity
            );
            return;
        }

        let expected_byte_count = quantity as usize * 2;
        if byte_count as usize != expected_byte_count || values.len() != expected_byte_count {
            server_log_debug!(
                "FC10 broadcast ignored due to invalid byte count: txn_id={}, byte_count={}, expected={}",
                txn_id,
                byte_count,
                expected_byte_count
            );
            return;
        }

        let mut registers = [0u16; FC16_MAX_QUANTITY as usize];
        for (index, chunk) in values.chunks_exact(2).enumerate() {
            registers[index] = u16::from_be_bytes([chunk[0], chunk[1]]);
        }

        if let Err(err) = self.app.write_multiple_registers_request(
            txn_id,
            unit_id_or_slave_addr,
            address,
            &registers[..quantity as usize],
        ) {
            server_log_debug!(
                "FC10 broadcast app callback failed: txn_id={}, unit_id_or_slave_addr={}, error={:?}",
                txn_id,
                unit_id_or_slave_addr.get(),
                err
            );
        }
    }
}
