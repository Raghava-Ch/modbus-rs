//! Modbus Discrete Inputs Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to Discrete Inputs (Function Code 0x02).
//!
//! It includes functionality for:
//! - Reading multiple or single discrete inputs.
//! - Packing and unpacking input states into bit-fields within bytes.
//! - Validating and parsing response PDUs from Modbus servers.
//!
//! This module is designed for `no_std` environments using `heapless` collections.

use heapless::Vec;
use mbus_core::{
    data_unit::common::{MAX_PDU_DATA_LEN, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
};

/// Provides operations for creating and parsing Modbus discrete input request/response PDUs.
pub(super) struct ReqPduCompiler {}

impl ReqPduCompiler {
    /// Creates a Modbus PDU for a Read Discrete Inputs (FC 0x02) request.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first input to read (0-65535).
    /// * `quantity` - The number of inputs to read (1-2000).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity is out of the valid Modbus range (1 to 2000).
    pub(super) fn read_discrete_inputs_request(
        address: u16,
        quantity: u16,
    ) -> Result<Pdu, MbusError> {
        if !(1..=2000).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadDiscreteInputs, data_vec, 4))
    }
}
