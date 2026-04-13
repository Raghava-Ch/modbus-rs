//! # Modbus PDU Framing Utilities
//!
//! Shared protocol encoding / decoding helpers used by both the coil and
//! register service sub-modules.  These utilities are independent of any
//! feature flag and live here so neither sub-module must depend on the other.

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN, ModbusMessage, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

/// Parses read-window requests with a two-field payload:
/// start address + quantity.
pub(super) fn parse_read_window(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    Ok((
        message.pdu.address_read_frame()?,
        message.pdu.quantity_from_read_frame()?,
    ))
}

/// Parses FC05/FC06 write-single requests.
///
/// Payload layout: address (u16 big-endian), value (u16 big-endian).
#[cfg(feature = "holding-registers")]
pub(super) fn parse_write_single_request(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    if message.pdu.data_len() != 4 {
        return Err(MbusError::InvalidPduLength);
    }
    let data = message.pdu.data();
    Ok((
        u16::from_be_bytes([data[0], data[1]]),
        u16::from_be_bytes([data[2], data[3]]),
    ))
}

/// Parses FC15/FC16 write-multiple requests.
///
/// Payload layout: address (u16), quantity (u16), byte_count (u8), values.
/// The parser verifies that total PDU data length matches `byte_count`.
#[cfg(feature = "holding-registers")]
pub(super) fn parse_write_multiple_request(
    message: &ModbusMessage,
) -> Result<(u16, u16, u8, &[u8]), MbusError> {
    if message.pdu.data_len() < 5 {
        return Err(MbusError::InvalidPduLength);
    }

    let data = message.pdu.data();
    let address = u16::from_be_bytes([data[0], data[1]]);
    let quantity = u16::from_be_bytes([data[2], data[3]]);
    let byte_count = data[4];
    let expected_len = 5usize
        .checked_add(byte_count as usize)
        .ok_or(MbusError::InvalidByteCount)?;
    if message.pdu.data_len() as usize != expected_len {
        return Err(MbusError::InvalidByteCount);
    }

    Ok((address, quantity, byte_count, &data[5..expected_len]))
}

/// Builds a read-style response payload: byte_count + raw payload bytes.
pub(super) fn build_byte_count_prefixed_response<TRANSPORT: Transport>(
    transport: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    payload: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let byte_count = u8::try_from(payload.len()).map_err(|_| MbusError::InvalidByteCount)?;
    let mut data = Vec::<u8, MAX_PDU_DATA_LEN>::new();
    data.push(byte_count)
        .map_err(|_| MbusError::BufferTooSmall)?;
    data.extend_from_slice(payload)
        .map_err(|_| MbusError::BufferTooSmall)?;
    build_response_frame(
        transport,
        txn_id,
        unit_id_or_slave_addr,
        function_code,
        data,
        byte_count.saturating_add(1),
    )
}

/// Builds a write-style echo response containing two `u16` values.
#[cfg(feature = "holding-registers")]
pub(super) fn build_echo_u16_response<TRANSPORT: Transport>(
    transport: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    first: u16,
    second: u16,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let mut data = Vec::<u8, MAX_PDU_DATA_LEN>::new();
    data.extend_from_slice(&first.to_be_bytes())
        .map_err(|_| MbusError::BufferTooSmall)?;
    data.extend_from_slice(&second.to_be_bytes())
        .map_err(|_| MbusError::BufferTooSmall)?;
    build_response_frame(
        transport,
        txn_id,
        unit_id_or_slave_addr,
        function_code,
        data,
        4,
    )
}

/// Parses FC16 (Mask Write Register) requests.
///
/// Payload layout: address (u16), and_mask (u16), or_mask (u16).
#[cfg(feature = "holding-registers")]
pub(super) fn parse_mask_write_request(
    message: &ModbusMessage,
) -> Result<(u16, u16, u16), MbusError> {
    if message.pdu.data_len() != 6 {
        return Err(MbusError::InvalidPduLength);
    }
    let data = message.pdu.data();
    Ok((
        u16::from_be_bytes([data[0], data[1]]),
        u16::from_be_bytes([data[2], data[3]]),
        u16::from_be_bytes([data[4], data[5]]),
    ))
}

/// Builds an FC16 mask-write echo response containing address, and-mask, and or-mask.
#[cfg(feature = "holding-registers")]
pub(super) fn build_mask_write_echo_response<TRANSPORT: Transport>(
    transport: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    address: u16,
    and_mask: u16,
    or_mask: u16,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let mut data = Vec::<u8, MAX_PDU_DATA_LEN>::new();
    data.extend_from_slice(&address.to_be_bytes())
        .map_err(|_| MbusError::BufferTooSmall)?;
    data.extend_from_slice(&and_mask.to_be_bytes())
        .map_err(|_| MbusError::BufferTooSmall)?;
    data.extend_from_slice(&or_mask.to_be_bytes())
        .map_err(|_| MbusError::BufferTooSmall)?;

    build_response_frame(
        transport,
        txn_id,
        unit_id_or_slave_addr,
        FunctionCode::MaskWriteRegister,
        data,
        6,
    )
}

/// Compiles a complete ADU frame from a function code and PDU payload.
fn build_response_frame<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    data: Vec<u8, MAX_PDU_DATA_LEN>,
    data_len: u8,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::new(function_code, data, data_len);
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}
