//! Request frame encoders — one free function per Modbus function code.
//!
//! Each function accepts user-supplied parameters plus the transaction id
//! assigned by the task, builds the corresponding PDU via `mbus_core` helpers,
//! and wraps it into a complete ADU frame ready to hand to `AsyncTransport::send`.
//!
//! # Conventions
//! - All functions return `Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError>`.
//! - `txn_id` is always the task-assigned identifier; callers never pick it.
//! - `unit` is the `UnitIdOrSlaveAddr` already validated before this point.
//! - Broadcast reads are rejected at the `client_core` layer; no guards here.

use heapless::Vec;

use mbus_core::{
    data_unit::common::{self, Pdu, MAX_ADU_FRAME_LEN},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{TransportType, UnitIdOrSlaveAddr},
};

#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::SubRequest;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};

use crate::client::command::ClientRequest;


// ─── Coils (FC 01 / 05 / 0F) ─────────────────────────────────────────────────

#[cfg(feature = "coils")]
/// Encodes a Read Multiple Coils (FC 01) request frame.
pub(crate) fn encode_read_coils(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if !(1..=2000).contains(&quantity) {
        return Err(MbusError::InvalidQuantity);
    }
    let pdu = Pdu::build_read_window(FunctionCode::ReadCoils, address, quantity)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "coils")]
/// Encodes a Write Single Coil (FC 05) request frame.
pub(crate) fn encode_write_single_coil(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    value: bool,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let coil_value: u16 = if value { 0xFF00 } else { 0x0000 };
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleCoil, address, coil_value)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "coils")]
/// Encodes a Write Multiple Coils (FC 0F) request frame.
pub(crate) fn encode_write_multiple_coils(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    coils: &Coils,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let quantity = coils.quantity();
    if !(1..=1968).contains(&quantity) {
        return Err(MbusError::InvalidPduLength);
    }
    let byte_count = quantity.div_ceil(8) as usize;
    let pdu = Pdu::build_write_multiple(
        FunctionCode::WriteMultipleCoils,
        address,
        quantity,
        &coils.values()[..byte_count],
    )?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

// ─── Registers (FC 03 / 04 / 06 / 10 / 16 / 17) ─────────────────────────────

#[cfg(feature = "registers")]
/// Encodes a Read Holding Registers (FC 03) request frame.
pub(crate) fn encode_read_holding_registers(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if !(1..=125).contains(&quantity) {
        return Err(MbusError::InvalidQuantity);
    }
    let pdu = Pdu::build_read_window(FunctionCode::ReadHoldingRegisters, address, quantity)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "registers")]
/// Encodes a Read Input Registers (FC 04) request frame.
pub(crate) fn encode_read_input_registers(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if !(1..=125).contains(&quantity) {
        return Err(MbusError::InvalidQuantity);
    }
    let pdu = Pdu::build_read_window(FunctionCode::ReadInputRegisters, address, quantity)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "registers")]
/// Encodes a Write Single Register (FC 06) request frame.
pub(crate) fn encode_write_single_register(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    value: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_write_single_u16(FunctionCode::WriteSingleRegister, address, value)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "registers")]
/// Encodes a Write Multiple Registers (FC 10) request frame.
pub(crate) fn encode_write_multiple_registers(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    values: &[u16],
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let quantity = values.len() as u16;
    if !(1..=123).contains(&quantity) {
        return Err(MbusError::InvalidQuantity);
    }
    let byte_pairs: Vec<u8, { MAX_ADU_FRAME_LEN }> = values
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let pdu = Pdu::build_write_multiple(
        FunctionCode::WriteMultipleRegisters,
        address,
        quantity,
        &byte_pairs,
    )?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "registers")]
/// Encodes a Read/Write Multiple Registers (FC 17) request frame.
pub(crate) fn encode_read_write_multiple_registers(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    read_address: u16,
    read_quantity: u16,
    write_address: u16,
    write_values: &[u16],
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let write_quantity = write_values.len() as u16;
    let byte_pairs: Vec<u8, { MAX_ADU_FRAME_LEN }> = write_values
        .iter()
        .flat_map(|v| v.to_be_bytes())
        .collect();
    let pdu = Pdu::build_read_write_multiple(
        read_address,
        read_quantity,
        write_address,
        write_quantity,
        &byte_pairs,
    )?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "registers")]
/// Encodes a Mask Write Register (FC 16) request frame.
pub(crate) fn encode_mask_write_register(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    and_mask: u16,
    or_mask: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_mask_write_register(address, and_mask, or_mask)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

// ─── Discrete inputs (FC 02) ─────────────────────────────────────────────────

#[cfg(feature = "discrete-inputs")]
/// Encodes a Read Discrete Inputs (FC 02) request frame.
pub(crate) fn encode_read_discrete_inputs(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    if !(1..=2000).contains(&quantity) {
        return Err(MbusError::InvalidQuantity);
    }
    let pdu = Pdu::build_read_window(FunctionCode::ReadDiscreteInputs, address, quantity)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

// ─── FIFO queue (FC 18) ───────────────────────────────────────────────────────

#[cfg(feature = "fifo")]
/// Encodes a Read FIFO Queue (FC 18) request frame.
pub(crate) fn encode_read_fifo_queue(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_u16_payload(FunctionCode::ReadFifoQueue, address)?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

// ─── File record (FC 14 / 15) ─────────────────────────────────────────────────

#[cfg(feature = "file-record")]
/// Encodes a Read File Record (FC 14) request frame.
pub(crate) fn encode_read_file_record(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    sub_request: &SubRequest,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    use mbus_core::models::file_record::PduDataBytes;
    let payload_bytes = sub_request.to_sub_req_pdu_bytes()?;
    // `to_sub_req_pdu_bytes` already prepends the Modbus byte-count field;
    // do NOT pass through `build_byte_count_payload` (that would add a second one).
    let data_len = payload_bytes.len() as u8;
    let pdu = Pdu::new(FunctionCode::ReadFileRecord, payload_bytes, data_len);
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "file-record")]
/// Encodes a Write File Record (FC 15) request frame.
pub(crate) fn encode_write_file_record(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    sub_request: &SubRequest,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    use mbus_core::models::file_record::PduDataBytes;
    let payload_bytes = sub_request.to_sub_req_pdu_bytes()?;
    // `to_sub_req_pdu_bytes` already prepends the Modbus byte-count field;
    // do NOT pass through `build_byte_count_payload` (that would add a second one).
    let data_len = payload_bytes.len() as u8;
    let pdu = Pdu::new(FunctionCode::WriteFileRecord, payload_bytes, data_len);
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

// ─── Diagnostics (FC 07 / 08 / 0B / 0C / 11 / 2B) ───────────────────────────

#[cfg(feature = "diagnostics")]
/// Encodes a Read Device Identification (FC 43 / MEI 0E) request frame.
pub(crate) fn encode_read_device_identification(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    read_device_id_code: ReadDeviceIdCode,
    object_id: ObjectId,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let object_id_byte = u8::from(object_id);
    let payload: [u8; 2] = [read_device_id_code as u8, object_id_byte];
    let pdu = Pdu::build_mei_type(
        FunctionCode::EncapsulatedInterfaceTransport,
        EncapsulatedInterfaceType::ReadDeviceIdentification as u8,
        &payload,
    )?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a generic Encapsulated Interface Transport (FC 43) request frame.
pub(crate) fn encode_encapsulated_interface_transport(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    mei_type: EncapsulatedInterfaceType,
    data: &[u8],
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_mei_type(
        FunctionCode::EncapsulatedInterfaceTransport,
        mei_type as u8,
        data,
    )?;
    common::compile_adu_frame(txn_id, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a Read Exception Status (FC 07) request frame (Serial only).
pub(crate) fn encode_read_exception_status(
    unit: UnitIdOrSlaveAddr,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_empty(FunctionCode::ReadExceptionStatus);
    // txn_id is unused on serial; pass 0
    common::compile_adu_frame(0, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a Diagnostics (FC 08) request frame (Serial only).
pub(crate) fn encode_diagnostics(
    unit: UnitIdOrSlaveAddr,
    sub_function: DiagnosticSubFunction,
    data: &[u16],
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_sub_function(FunctionCode::Diagnostics, sub_function as u16, data)?;
    common::compile_adu_frame(0, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a Get Comm Event Counter (FC 0B) request frame (Serial only).
pub(crate) fn encode_get_comm_event_counter(
    unit: UnitIdOrSlaveAddr,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_empty(FunctionCode::GetCommEventCounter);
    common::compile_adu_frame(0, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a Get Comm Event Log (FC 0C) request frame (Serial only).
pub(crate) fn encode_get_comm_event_log(
    unit: UnitIdOrSlaveAddr,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_empty(FunctionCode::GetCommEventLog);
    common::compile_adu_frame(0, unit.get(), pdu, transport_type)
}

#[cfg(feature = "diagnostics")]
/// Encodes a Report Server ID (FC 11) request frame (Serial only).
pub(crate) fn encode_report_server_id(
    unit: UnitIdOrSlaveAddr,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    let pdu = Pdu::build_empty(FunctionCode::ReportServerId);
    common::compile_adu_frame(0, unit.get(), pdu, transport_type)
}

// ─── Top-level dispatcher ─────────────────────────────────────────────────────

/// Encodes a [`ClientRequest`] into a complete ADU frame using the given `txn_id`.
///
/// For serial-only function codes (FC 07 / 08 / 0B / 0C / 11) the `txn_id` is
/// passed through but the serial ADU format does not include it on the wire.
///
/// [`ClientRequest`]: crate::client::command::ClientRequest
pub(crate) fn encode_request(
    txn_id: u16,
    req: &ClientRequest,
    transport_type: TransportType,
) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
    match req {
        #[cfg(feature = "coils")]
        ClientRequest::ReadMultipleCoils { unit, address, quantity } =>
            encode_read_coils(txn_id, *unit, *address, *quantity, transport_type),

        #[cfg(feature = "coils")]
        ClientRequest::WriteSingleCoil { unit, address, value } =>
            encode_write_single_coil(txn_id, *unit, *address, *value, transport_type),

        #[cfg(feature = "coils")]
        ClientRequest::WriteMultipleCoils { unit, address, coils } =>
            encode_write_multiple_coils(txn_id, *unit, *address, coils, transport_type),

        #[cfg(feature = "registers")]
        ClientRequest::ReadHoldingRegisters { unit, address, quantity } =>
            encode_read_holding_registers(txn_id, *unit, *address, *quantity, transport_type),

        #[cfg(feature = "registers")]
        ClientRequest::ReadInputRegisters { unit, address, quantity } =>
            encode_read_input_registers(txn_id, *unit, *address, *quantity, transport_type),

        #[cfg(feature = "registers")]
        ClientRequest::WriteSingleRegister { unit, address, value } =>
            encode_write_single_register(txn_id, *unit, *address, *value, transport_type),

        #[cfg(feature = "registers")]
        ClientRequest::WriteMultipleRegisters { unit, address, values } =>
            encode_write_multiple_registers(txn_id, *unit, *address, values, transport_type),

        #[cfg(feature = "registers")]
        ClientRequest::ReadWriteMultipleRegisters {
            unit, read_address, read_quantity, write_address, write_values,
        } => encode_read_write_multiple_registers(
            txn_id, *unit, *read_address, *read_quantity, *write_address,
            write_values, transport_type,
        ),

        #[cfg(feature = "registers")]
        ClientRequest::MaskWriteRegister { unit, address, and_mask, or_mask } =>
            encode_mask_write_register(txn_id, *unit, *address, *and_mask, *or_mask, transport_type),

        #[cfg(feature = "discrete-inputs")]
        ClientRequest::ReadDiscreteInputs { unit, address, quantity } =>
            encode_read_discrete_inputs(txn_id, *unit, *address, *quantity, transport_type),

        #[cfg(feature = "fifo")]
        ClientRequest::ReadFifoQueue { unit, address } =>
            encode_read_fifo_queue(txn_id, *unit, *address, transport_type),

        #[cfg(feature = "file-record")]
        ClientRequest::ReadFileRecord { unit, sub_request } =>
            encode_read_file_record(txn_id, *unit, sub_request, transport_type),

        #[cfg(feature = "file-record")]
        ClientRequest::WriteFileRecord { unit, sub_request } =>
            encode_write_file_record(txn_id, *unit, sub_request, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::ReadDeviceIdentification { unit, read_device_id_code, object_id } =>
            encode_read_device_identification(
                txn_id, *unit, *read_device_id_code, *object_id, transport_type,
            ),

        #[cfg(feature = "diagnostics")]
        ClientRequest::EncapsulatedInterfaceTransport { unit, mei_type, data } =>
            encode_encapsulated_interface_transport(txn_id, *unit, *mei_type, data, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::ReadExceptionStatus { unit } =>
            encode_read_exception_status(*unit, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::Diagnostics { unit, sub_function, data } =>
            encode_diagnostics(*unit, *sub_function, data, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::GetCommEventCounter { unit } =>
            encode_get_comm_event_counter(*unit, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::GetCommEventLog { unit } =>
            encode_get_comm_event_log(*unit, transport_type),

        #[cfg(feature = "diagnostics")]
        ClientRequest::ReportServerId { unit } =>
            encode_report_server_id(*unit, transport_type),
    }
}
