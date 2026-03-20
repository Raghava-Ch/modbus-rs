//! Modbus Coils Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to Coils (Function Codes 0x01, 0x05, and 0x0F).
//!
//! It includes functionality for:
//! - Reading multiple or single coils.
//! - Writing single or multiple coils.
//! - Packing and unpacking coil states into bit-fields within bytes.

use heapless::Vec;
use mbus_core::data_unit::common::{MAX_PDU_DATA_LEN, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;

use core::usize;

/// Provides operations for reading and writing Modbus coils.
///
/// This struct is stateless and provides static methods to create request PDUs
/// and parse response PDUs for coil-related Modbus function codes.
pub(super) struct ReqPduCompiler {}

/// Provides operations for reading and writing Modbus coils, as well as parsing responses for coil-related function codes.
impl ReqPduCompiler {
    /// Creates a Modbus PDU for a Read Coils (FC 0x01) request.
    ///
    /// This function constructs the PDU required to read the ON/OFF status of
    /// a contiguous block of coils from a Modbus server.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first coil to read (0-65535).
    /// * `quantity` - The number of coils to read (1-2000).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity is out of the valid Modbus range (1 to 2000).
    pub(super) fn read_coils_request(address: u16, quantity: u16) -> Result<Pdu, MbusError> {
        if !(1..=2000).contains(&quantity) {
            return Err(MbusError::InvalidPduLength); // Quantity out of range
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(
            FunctionCode::ReadCoils,
            data_vec,
            4, // 2 bytes for address, 2 bytes for quantity
        ))
    }

    /// Creates a Modbus PDU for a Write Single Coil (FC 0x05) request.
    ///
    /// This function constructs the PDU required to force a single coil to
    /// either ON (0xFF00) or OFF (0x0000) state.
    ///
    /// # Arguments
    /// * `address` - The address of the coil to write (0-65535).
    /// * `value` - The state to write to the coil (`true` for ON, `false` for OFF).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub(super) fn write_single_coil_request(address: u16, value: bool) -> Result<Pdu, MbusError> {
        macro_rules! push_be {
            ($vec:expr, $val:expr) => {
                $vec.extend_from_slice(&$val.to_be_bytes())
                    .map_err(|_| MbusError::BufferLenMissmatch)
            };
        }

        let mut data_bytes: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        push_be!(data_bytes, address)?;

        // Modbus protocol uses 0xFF00 for ON and 0x0000 for OFF
        let coil_value: u16 = if value { 0xFF00 } else { 0x0000 };
        push_be!(data_bytes, coil_value)?;

        Ok(Pdu::new(
            FunctionCode::WriteSingleCoil,
            data_bytes,
            4, // 2 bytes for address, 2 bytes for value
        ))
    }

    /// Creates a Modbus PDU for a Write Multiple Coils (FC 0x0F) request.
    ///
    /// This function constructs the PDU required to force a contiguous block of
    /// coils to specific ON/OFF states.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first coil to write (0-65535).
    /// * `quantity` - The number of coils to write (1-1968).
    /// * `values` - A slice of booleans representing the coil states to write.
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity or the length of `values` is invalid.
    pub(super) fn write_multiple_coils_request(
        address: u16,
        quantity: u16,
        values: &[bool],
    ) -> Result<Pdu, MbusError> {
        // Max quantity for Write Multiple Coils is 1968.
        // PDU data: Address (2 bytes) + Quantity (2 bytes) + Byte Count (1 byte) + Coil Status (N bytes)
        // Max PDU data length is 252.
        // 2 + 2 + 1 + ceil(1968/8) = 5 + 246 = 251 bytes. This fits.
        if !(1..=1968).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        if values.len() as u16 != quantity {
            return Err(MbusError::InvalidPduLength); // Mismatch between quantity and values length
        }

        let byte_count = ((quantity + 7) / 8) as u8;
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();

        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .push(byte_count)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Initialize bytes for coil data
        let num_coil_bytes = byte_count as usize;
        data_vec
            .resize(data_vec.len() + num_coil_bytes, 0)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for (i, &value) in values.iter().enumerate() {
            if value {
                let byte_index = 5 + (i / 8); // Offset by 5 (addr, qty, byte_count) in the PDU data
                let bit_index = i % 8;
                data_vec[byte_index] |= 1 << bit_index;
            }
        }

        Ok(Pdu::new(
            FunctionCode::WriteMultipleCoils,
            data_vec,
            5 + byte_count as u8, // 2 bytes addr + 2 bytes qty + 1 byte byte_count + N bytes coil data
        ))
    }
}
