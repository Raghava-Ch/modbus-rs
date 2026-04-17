//! # Modbus PDU Framing Utilities
//!
//! Shared protocol encoding / decoding helpers used by both the coil and
//! register service sub-modules.  These utilities are independent of any
//! feature flag and live here so neither sub-module must depend on the other.

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN, ModbusMessage, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::{
    FileRecordReadSubRequest, FileRecordWriteSubRequest, MAX_SUB_REQUESTS_PER_PDU,
};
use mbus_core::transport::{Transport, UnitIdOrSlaveAddr};

#[cfg(feature = "file-record")]
const FILE_RECORD_RESPONSE_MAX_PAYLOAD_LEN: usize = MAX_PDU_DATA_LEN - 1;

/// Parses read-window requests with a two-field payload:
/// start address + quantity.
pub(super) fn parse_read_window(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    let rw = message.pdu.read_window()?;
    Ok((rw.address, rw.quantity))
}

/// Parses request PDUs that must have an empty payload (0 bytes).
pub(super) fn parse_empty_request(message: &ModbusMessage) -> Result<(), MbusError> {
    if message.pdu.data_len() != 0 {
        return Err(MbusError::InvalidPduLength);
    }
    Ok(())
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
    Ok((
        fields.address,
        fields.quantity,
        fields.byte_count,
        fields.values,
    ))
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

/// Builds a response carrying exactly one data byte.
pub(super) fn build_single_byte_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    value: u8,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_byte_payload(function_code, value)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Builds a response carrying exactly two `u16` values (big-endian).
pub(super) fn build_two_u16_response<TRANSPORT: Transport>(
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

/// Parses FC17 (Read/Write Multiple Registers) requests.
///
/// Payload layout: read_address (u16), read_quantity (u16), write_address (u16),
/// write_quantity (u16), write_byte_count (u8), write_values.
#[cfg(feature = "holding-registers")]
pub(super) fn parse_read_write_multiple_request(
    message: &ModbusMessage,
) -> Result<common::ReadWriteMultipleFields<'_>, MbusError> {
    message.pdu.read_write_multiple_fields()
}

/// Parses FC14 (Read File Record) requests into validated sub-requests.
///
/// Request PDU data layout:
/// - byte_count (1)
/// - repeated sub-requests (7 bytes each):
///   - reference_type (1) = 0x06
///   - file_number (2)
///   - record_number (2)
///   - record_length (2)
#[cfg(feature = "file-record")]
pub(super) fn parse_file_record_read_request(
    message: &ModbusMessage,
) -> Result<Vec<FileRecordReadSubRequest, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
    message.pdu.file_record_read_sub_requests()
}

/// Parses FC15 (Write File Record) requests into validated sub-requests.
///
/// Request PDU data layout:
/// - byte_count (1)
/// - repeated sub-requests:
///   - reference_type (1) = 0x06
///   - file_number (2)
///   - record_number (2)
///   - record_length (2)
///   - record_data (record_length * 2)
#[cfg(feature = "file-record")]
pub(super) fn parse_file_record_write_request(
    message: &ModbusMessage,
) -> Result<Vec<FileRecordWriteSubRequest<'_>, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
    message.pdu.file_record_write_sub_requests()
}

/// Builds an FC14 (Read File Record) response frame.
///
/// `payload` must contain only the concatenated sub-response blocks:
/// `[sub_len, ref_type, data..]...`
/// This helper prepends the FC14 response `byte_count` field.
#[cfg(feature = "file-record")]
pub(super) fn build_file_record_read_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    payload: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if payload.len() > FILE_RECORD_RESPONSE_MAX_PAYLOAD_LEN {
        return Err(MbusError::FileReadPduOverflow);
    }
    let pdu = Pdu::build_byte_count_payload(FunctionCode::ReadFileRecord, payload)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Builds an FC15 (Write File Record) echo response.
///
/// FC15 success response echoes the request PDU data exactly.
#[cfg(feature = "file-record")]
pub(super) fn build_file_record_write_echo_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    request_pdu_data: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if request_pdu_data.len() > MAX_PDU_DATA_LEN {
        return Err(MbusError::BufferTooSmall);
    }
    let mut data: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
    data.extend_from_slice(request_pdu_data)
        .map_err(|_| MbusError::BufferLenMissmatch)?;
    let pdu = Pdu::new(
        FunctionCode::WriteFileRecord,
        data,
        request_pdu_data.len() as u8,
    );
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Parses FC18 (Read FIFO Queue) requests.
///
/// Payload layout: pointer_address (u16, big-endian) — exactly 2 bytes.
/// Returns the FIFO pointer address.
#[cfg(feature = "fifo")]
pub(super) fn parse_read_fifo_pointer_request(message: &ModbusMessage) -> Result<u16, MbusError> {
    message.pdu.fifo_pointer()
}

/// Parses FC08 (Diagnostics) requests.
///
/// Payload layout: sub-function (u16, 2 bytes) + data (u16, 2 bytes).
/// Returns (sub_function_code, data_word).
#[cfg(feature = "diagnostics")]
pub(super) fn parse_diagnostics_request(message: &ModbusMessage) -> Result<(u16, u16), MbusError> {
    message.pdu.diagnostics_fields()
}

/// Builds a FC18 (Read FIFO Queue) response frame.
///
/// Response PDU layout: byte_count(2, BE) | fifo_count(2, BE) | values.
/// `app_payload` must be the bytes written by the app callback:
/// `fifo_count(2 bytes BE) + values`, i.e. exactly `2 + fifo_count * 2` bytes.
#[cfg(feature = "fifo")]
pub(super) fn build_fifo_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    app_payload: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_fifo_payload(app_payload)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Builds a response for FC08 (Diagnostics) requests.
///
/// Echo sub-function (2 bytes) + result data (2 bytes).
#[cfg(feature = "diagnostics")]
pub(super) fn build_diagnostics_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    sub_function: u16,
    result: u16,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_diagnostics(sub_function, result)?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Parses an FC2B (Encapsulated Interface Transport) request PDU.
///
/// Returns `MeiTypePayload { mei_type_byte, payload }` where `payload` is the
/// data following the MEI type byte (e.g. `[read_device_id_code, start_object_id]`
/// for MEI 0x0E).
#[cfg(feature = "diagnostics")]
pub(super) fn parse_fc2b_request(
    message: &ModbusMessage,
) -> Result<common::MeiTypePayload<'_>, MbusError> {
    message.pdu.mei_type_payload()
}

/// Builds an FC2B / MEI 0x0E (Read Device Identification) response ADU.
///
/// `objects_payload` must be a well-formed sequence of `[id(1), len(1), value(N)…]` triples
/// exactly as written by the application callback. The number of objects is computed by
/// walking the triples so the caller does not need to count them separately.
#[cfg(feature = "diagnostics")]
#[allow(clippy::too_many_arguments)]
pub(super) fn build_fc2b_read_device_id_response<TRANSPORT: Transport>(
    _: &TRANSPORT,
    txn_id: u16,
    unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    read_device_id_code: u8,
    conformity_level: u8,
    more_follows: bool,
    next_object_id: u8,
    objects_payload: &[u8],
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    use mbus_core::function_codes::public::EncapsulatedInterfaceType;

    let n_objects = count_fc2b_objects(objects_payload)?;
    let more_byte: u8 = if more_follows { 0xFF } else { 0x00 };

    // MEI data passed to build_mei_type (after the 0x0E byte):
    // [code(1), conformity(1), more(1), next_id(1), n_objects(1), ...objects...]
    let header = [
        read_device_id_code,
        conformity_level,
        more_byte,
        next_object_id,
        n_objects,
    ];
    let mei_data_len = header.len() + objects_payload.len();
    if mei_data_len > MAX_PDU_DATA_LEN - 1 {
        // -1 for the MEI type byte itself
        return Err(MbusError::BufferTooSmall);
    }
    let mut mei_data: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
    mei_data
        .extend_from_slice(&header)
        .map_err(|_| MbusError::BufferTooSmall)?;
    mei_data
        .extend_from_slice(objects_payload)
        .map_err(|_| MbusError::BufferTooSmall)?;

    let pdu = Pdu::build_mei_type(
        FunctionCode::EncapsulatedInterfaceTransport,
        EncapsulatedInterfaceType::ReadDeviceIdentification as u8,
        &mei_data,
    )?;
    common::compile_adu_frame(
        txn_id,
        unit_id_or_slave_addr.get(),
        pdu,
        TRANSPORT::TRANSPORT_TYPE,
    )
}

/// Counts the number of `[id(1), len(1), value(N)…]` object triples in `payload`.
#[cfg(feature = "diagnostics")]
fn count_fc2b_objects(payload: &[u8]) -> Result<u8, MbusError> {
    let mut offset = 0usize;
    let mut count: u8 = 0;
    while offset < payload.len() {
        if offset + 2 > payload.len() {
            return Err(MbusError::InvalidPduLength);
        }
        let val_len = payload[offset + 1] as usize;
        offset += 2 + val_len;
        if offset > payload.len() {
            return Err(MbusError::InvalidPduLength);
        }
        count = count.checked_add(1).ok_or(MbusError::InvalidPduLength)?;
    }
    Ok(count)
}
