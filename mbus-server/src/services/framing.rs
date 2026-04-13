//! # Modbus PDU Framing Utilities
//!
//! Shared protocol encoding / decoding helpers used by both the coil and
//! register service sub-modules.  These utilities are independent of any
//! feature flag and live here so neither sub-module must depend on the other.

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, ModbusMessage, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

/// Parses read-window requests with a two-field payload:
/// start address + quantity.
pub(super) fn parse_read_window(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    let rw = message.pdu.read_window()?;
    Ok((rw.address, rw.quantity))
}

/// Parses FC05/FC06 write-single requests.
///
/// Payload layout: address (u16 big-endian), value (u16 big-endian).
#[cfg(feature = "holding-registers")]
pub(super) fn parse_write_single_request(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    let fields = message.pdu.write_single_u16_fields()?;
    Ok((fields.address, fields.value))
}

/// Parses FC15/FC16 write-multiple requests.
///
/// Payload layout: address (u16), quantity (u16), byte_count (u8), values.
/// The parser verifies that total PDU data length matches `byte_count`.
#[cfg(feature = "holding-registers")]
pub(super) fn parse_write_multiple_request(
    message: &ModbusMessage,
) -> Result<(u16, u16, u8, &[u8]), MbusError> {
    let fields = message.pdu.write_multiple_fields()?;
    Ok((fields.address, fields.quantity, fields.byte_count, fields.values))
}

/// Builds a read-style response payload: byte_count + raw payload bytes.
pub(super) fn build_byte_count_prefixed_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    payload: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_byte_count_payload(function_code, payload)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Builds a write-style echo response containing two `u16` values.
#[cfg(feature = "holding-registers")]
pub(super) fn build_echo_u16_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    first: u16,
    second: u16,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_write_single_u16(function_code, first, second)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Parses FC16 (Mask Write Register) requests.
///
/// Payload layout: address (u16), and_mask (u16), or_mask (u16).
#[cfg(feature = "holding-registers")]
pub(super) fn parse_mask_write_request(
    message: &ModbusMessage,
) -> Result<(u16, u16, u16), MbusError> {
    let fields = message.pdu.mask_write_register_fields()?;
    Ok((fields.address, fields.and_mask, fields.or_mask))
}

/// Builds an FC16 mask-write echo response containing address, and-mask, and or-mask.
#[cfg(feature = "holding-registers")]
pub(super) fn build_mask_write_echo_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_mask_write_register(address, and_mask, or_mask)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

