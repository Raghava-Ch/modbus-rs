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

use mbus_core::{
    data_unit::common::{MAX_PDU_DATA_LEN, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
};

use heapless::Vec;

/// Provides operations for reading and writing Modbus registers.
pub(super) struct ReqPduCompiler {}

/// The `RegReqPdu` struct provides methods to create Modbus PDUs for various register operations, including:
/// - Reading holding registers (FC 0x03)
/// - Reading input registers (FC 0x04)
/// - Writing a single register (FC 0x06)
/// - Writing multiple registers (FC 0x10)
/// - Read/Write multiple registers (FC 0x17)
/// - Mask write register (FC 0x16)
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

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadHoldingRegisters, data_vec, 4))
    }

    /// Creates a Modbus PDU for a Read Input Registers (FC 0x04) request.
    pub(super) fn read_input_registers_request(
        address: u16,
        quantity: u16,
    ) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadInputRegisters, data_vec, 4))
    }

    /// Creates a Modbus PDU for a Write Single Register (FC 0x06) request.
    pub(super) fn write_single_register_request(
        address: u16,
        value: u16,
    ) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&value.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::WriteSingleRegister, data_vec, 4))
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
            return Err(MbusError::InvalidPduLength); // Mismatch between quantity and values length
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Add byte count (2 bytes per register)
        let byte_count = quantity * 2;
        data_vec
            .push(byte_count as u8)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for &value in values {
            data_vec
                .extend_from_slice(&value.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        let data_len = data_vec.len() as u8;
        Ok(Pdu::new(
            FunctionCode::WriteMultipleRegisters,
            data_vec,
            data_len,
        ))
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
        let write_quantity = write_values.len() as u16; // N
        if !(1..=121).contains(&write_quantity) {
            // Corrected max quantity for write_values in FC 0x17
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&read_address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&read_quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&write_address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&write_quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Add byte count for write values
        let byte_count = write_quantity * 2;
        data_vec
            .push(byte_count as u8)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for &value in write_values {
            data_vec
                .extend_from_slice(&value.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        Ok(Pdu::new(
            FunctionCode::ReadWriteMultipleRegisters,
            data_vec,
            9 + byte_count as u8, // 2 read_addr + 2 read_qty + 2 write_addr + 2 write_qty + 1 byte_count + N*2 bytes
        ))
    }

    /// Creates a Modbus PDU for a Mask Write Register (FC 0x16) request.
    pub(super) fn mask_write_register_request(
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&and_mask.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&or_mask.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::MaskWriteRegister, data_vec, 6)) // Corrected: 2 addr + 2 and_mask + 2 or_mask
    }
}
