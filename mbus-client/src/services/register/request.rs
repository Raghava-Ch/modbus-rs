//! Modbus Registers Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to Holding and Input Registers.
//!
//! It includes functionality for:
//! - Reading holding registers (FC 0x03) and input registers (FC 0x04).
//! - Writing single registers (FC 0x06) and multiple registers (FC 0x10).
//! - Atomic Read/Write of multiple registers (FC 0x17).
//! - Masking bits in a single register (FC 0x16).
//! - Validating and parsing response PDUs from Modbus servers.
//!
//! This module is designed for `no_std` environments using `heapless` collections,
//! ensuring memory safety and predictability for embedded systems.

use mbus_core::{data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode};

/// Provides operations for reading and writing Modbus registers.
pub(super) struct ReqPduCompiler {}

/// The `RegReqPdu` struct provides methods to create Modbus PDUs for various register operations, including:
/// - Reading holding registers (FC 0x03)
/// - Reading input registers (FC 0x04)
/// - Writing a single register (FC 0x06)
/// - Writing multiple registers (FC 0x10)
/// - Read/Write multiple registers (FC 0x17)
/// - Mask write register (FC 0x16)
///
/// Each method validates the input parameters and constructs a PDU with the appropriate function code and data payload for the specified operation.
impl ReqPduCompiler {
    /// Creates a Modbus PDU for a Read Holding Registers (FC 0x03) request.
    pub(super) fn read_holding_registers_request(
        address: u16,
        quantity: u16,
    ) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        Pdu::build_read_window(FunctionCode::ReadHoldingRegisters, address, quantity)
    }

    /// Creates a Modbus PDU for a Read Input Registers (FC 0x04) request.
    pub(super) fn read_input_registers_request(
        address: u16,
        quantity: u16,
    ) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        Pdu::build_read_window(FunctionCode::ReadInputRegisters, address, quantity)
    }

    /// Creates a Modbus PDU for a Write Single Register (FC 0x06) request.
    pub(super) fn write_single_register_request(
        address: u16,
        value: u16,
    ) -> Result<Pdu, MbusError> {
        Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, address, value)
    }

    /// Creates a Modbus PDU for a Write Multiple Registers (FC 0x10) request.
    pub(super) fn write_multiple_registers_request(
        address: u16,
        quantity: u16,
        values: &[u16],
    ) -> Result<Pdu, MbusError> {
        if !(1..=123).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        if values.len() as u16 != quantity {
            return Err(MbusError::InvalidPduLength);
        }
        // Pack register words into bytes
        let mut packed: heapless::Vec<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }> =
            heapless::Vec::new();
        for &v in values {
            packed
                .extend_from_slice(&v.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }
        Pdu::build_write_multiple(
            FunctionCode::WriteMultipleRegisters,
            address,
            quantity,
            &packed,
        )
    }

    /// Creates a Modbus PDU for a Read/Write Multiple Registers (FC 0x17) request.
    pub(super) fn read_write_multiple_registers_request(
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&read_quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        let write_quantity = write_values.len() as u16;
        if !(1..=121).contains(&write_quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        // Pack register words into bytes
        let mut packed: heapless::Vec<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }> =
            heapless::Vec::new();
        for &v in write_values {
            packed
                .extend_from_slice(&v.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }
        Pdu::build_read_write_multiple(
            read_address,
            read_quantity,
            write_address,
            write_quantity,
            &packed,
        )
    }

    /// Creates a Modbus PDU for a Mask Write Register (FC 0x16) request.
    pub(super) fn mask_write_register_request(
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<Pdu, MbusError> {
        Pdu::build_mask_write_register(address, and_mask, or_mask)
    }
}
