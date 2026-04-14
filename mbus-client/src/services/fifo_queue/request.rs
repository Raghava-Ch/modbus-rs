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

use mbus_core::{data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode};

/// Provides operations for creating and parsing Modbus FIFO Queue request/response PDUs.
pub(super) struct ReqPduCompiler {}

impl ReqPduCompiler {
    /// Creates a Modbus Read FIFO Queue request PDU.
    pub(super) fn read_fifo_queue_request(address: u16) -> Result<Pdu, MbusError> {
        Pdu::build_u16_payload(FunctionCode::ReadFifoQueue, address)
    }
}
