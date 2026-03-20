//! Modbus FIFO Queue Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to FIFO Queues (Function Code 0x18).
//!
//! It includes functionality for:
//! - Reading the contents of a remote FIFO queue of registers.
//! - Parsing response PDUs containing the FIFO count and register values.
//! - Validating data integrity (byte counts vs register counts).
//!
//! This module is designed for `no_std` environments using `heapless` collections.
//! The maximum number of registers in a single FIFO response is limited to 31 by the protocol.

use heapless::Vec;

use mbus_core::{
    data_unit::common::{MAX_PDU_DATA_LEN, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
};

/// Provides operations for creating and parsing Modbus FIFO Queue request/response PDUs.
pub(super) struct ReqPduCompiler {}

impl ReqPduCompiler {
    /// Creates a Modbus Read FIFO Queue request PDU.
    pub(super) fn read_fifo_queue_request(address: u16) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadFifoQueue, data_vec, 2)) // Corrected: 2 addr
    }
}
