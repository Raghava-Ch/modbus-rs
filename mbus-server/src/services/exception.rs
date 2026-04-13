//! # Modbus Exception Response Handling
//!
//! This module provides centralized exception response encoding for Modbus function codes.
//! It builds exception ADUs and sends them via the transport layer.

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;

/// Builds a Modbus exception response ADU for the given function code and exception code.
///
/// An exception response has the structure:
/// - Function Code (with 0x80 bit set to indicate exception)
/// - Exception Code (1 byte)
///
/// This function constructs the entire ADU frame (MBAP header + exception PDU) ready
/// for transmission. The ADU format depends on the transport type (TCP vs. RTU/ASCII).
///
/// # Arguments
/// * `txn_id` - Transaction ID (used in TCP MBAP header)
/// * `unit_id_or_slave_addr` - Unit ID (TCP) or Slave Address (Serial)
/// * `function_code` - The original function code (will be converted to exception variant)
/// * `exception_code` - The Modbus exception code to send
/// * `transport_type` - The transport type (determines ADU format)
///
/// # Returns
/// A `Result` containing the complete ADU frame, or an error if encoding fails
pub fn build_exception_adu(
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    exception_code: ExceptionCode,
    transport_type: mbus_core::transport::TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    // Get the exception function code variant (with 0x80 bit already set)
    let exception_fc = function_code
        .exception_response()
        .ok_or(MbusError::InvalidFunctionCode)?;

    // Build PDU with exception code
    let pdu = Pdu::build_byte_payload(exception_fc, exception_code as u8)
        .map_err(|_| MbusError::Unexpected)?;

    // Compile the ADU with the exception function code (no bit manipulation needed)
    common::compile_adu_frame(txn_id, unit_id_or_slave_addr.get(), pdu, transport_type)
}
