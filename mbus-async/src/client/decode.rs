//! Response frame decoders — one free function per Modbus function code.
//!
//! # Entry point
//!
//! [`decode_response`] is the single entry called by the task receive branch.
//! It decompiles the raw ADU into a [`ModbusMessage`], extracts the transaction
//! id + unit, and dispatches to a per-FC helper that returns a [`ClientResponse`].
//!
//! # Error handling
//! - Exception PDUs (high bit set on FC byte) → `MbusError::ModbusException`.
//! - Unknown / unsupported FC → `MbusError::InvalidFunctionCode`.
//! - Malformed payload → the specific `MbusError` from the Pdu helper.
//!
//! # Note on stateless decoding
//! The async client does not keep per-request metadata (expected_quantity etc.)
//! because the task resolves responses by txn_id and the caller already knows
//! what it sent.  For responses that echo address+quantity back (FC 0F, 10, 16)
//! we accept whatever the server returns; validation happens in the typed
//! extractors in `client_core`.

use mbus_core::{
    data_unit::common::{Pdu, decompile_adu_frame},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::{TransportType, UnitIdOrSlaveAddr},
};

use crate::client::response::ClientResponse;

#[cfg(feature = "coils")]
use mbus_core::models::coil::Coils;
#[cfg(feature = "discrete-inputs")]
use mbus_core::models::discrete_input::DiscreteInputs;
#[cfg(feature = "fifo")]
use mbus_core::models::fifo_queue::FifoQueue;
#[cfg(feature = "file-record")]
use mbus_core::models::file_record::{
    FILE_RECORD_REF_TYPE, MAX_SUB_REQUESTS_PER_PDU, SubRequestParams,
};
#[cfg(feature = "registers")]
use mbus_core::models::register::Registers;
#[cfg(feature = "diagnostics")]
use mbus_core::{
    data_unit::common::MAX_PDU_DATA_LEN,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    models::diagnostic::{
        ConformityLevel, DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode,
    },
};
#[cfg(any(feature = "file-record", feature = "diagnostics"))]
use heapless::Vec;

// ─── Public entry point ───────────────────────────────────────────────────────

/// Decompiles a raw transport frame and decodes the response into a typed
/// `(txn_id, unit, ClientResponse)` triple.
///
/// Returns `Err(MbusError)` for any framing, exception, or parse error.
///
/// On an exception response the returned `Err` is wrapped in `Ok((txn_id, unit, Err(...)))`
/// so the caller can still route the error to the correct pending entry.
pub(crate) fn decode_response(
    frame: &[u8],
    transport_type: TransportType,
) -> Result<(u16, UnitIdOrSlaveAddr, Result<ClientResponse, MbusError>), MbusError> {
    let message = decompile_adu_frame(frame, transport_type)?;

    let txn_id = message.transaction_id();
    let unit = message.unit_id_or_slave_addr();
    let pdu = message.pdu();

    // Exception PDU: if error_code is Some, the server returned an exception
    // response.  The exception code byte is the payload.
    if let Some(exception_code) = pdu.error_code() {
        return Ok((
            txn_id,
            unit,
            Err(MbusError::ModbusException(exception_code)),
        ));
    }

    let response = decode_pdu(pdu)?;
    Ok((txn_id, unit, Ok(response)))
}

// ─── PDU dispatcher ───────────────────────────────────────────────────────────

/// Dispatches to the per-FC decoder based on the PDU's function code.
fn decode_pdu(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    match pdu.function_code() {
        #[cfg(feature = "coils")]
        FunctionCode::ReadCoils => decode_read_coils(pdu),
        #[cfg(feature = "coils")]
        FunctionCode::WriteSingleCoil => decode_write_single_coil(pdu),
        #[cfg(feature = "coils")]
        FunctionCode::WriteMultipleCoils => decode_write_multiple_coils(pdu),

        #[cfg(feature = "discrete-inputs")]
        FunctionCode::ReadDiscreteInputs => decode_read_discrete_inputs(pdu),

        #[cfg(feature = "registers")]
        FunctionCode::ReadHoldingRegisters => decode_read_registers(pdu),
        #[cfg(feature = "registers")]
        FunctionCode::ReadInputRegisters => decode_read_registers(pdu),
        #[cfg(feature = "registers")]
        FunctionCode::WriteSingleRegister => decode_write_single_register(pdu),
        #[cfg(feature = "registers")]
        FunctionCode::WriteMultipleRegisters => decode_write_multiple_registers(pdu),
        #[cfg(feature = "registers")]
        FunctionCode::ReadWriteMultipleRegisters => decode_read_registers(pdu),
        #[cfg(feature = "registers")]
        FunctionCode::MaskWriteRegister => decode_mask_write_register(pdu),

        #[cfg(feature = "fifo")]
        FunctionCode::ReadFifoQueue => decode_read_fifo_queue(pdu),

        #[cfg(feature = "file-record")]
        FunctionCode::ReadFileRecord => decode_read_file_record(pdu),
        #[cfg(feature = "file-record")]
        FunctionCode::WriteFileRecord => decode_write_file_record(pdu),

        #[cfg(feature = "diagnostics")]
        FunctionCode::EncapsulatedInterfaceTransport => {
            decode_encapsulated_interface_transport(pdu)
        }
        #[cfg(feature = "diagnostics")]
        FunctionCode::ReadExceptionStatus => decode_read_exception_status(pdu),
        #[cfg(feature = "diagnostics")]
        FunctionCode::Diagnostics => decode_diagnostics(pdu),
        #[cfg(feature = "diagnostics")]
        FunctionCode::GetCommEventCounter => decode_get_comm_event_counter(pdu),
        #[cfg(feature = "diagnostics")]
        FunctionCode::GetCommEventLog => decode_get_comm_event_log(pdu),
        #[cfg(feature = "diagnostics")]
        FunctionCode::ReportServerId => decode_report_server_id(pdu),

        _ => Err(MbusError::InvalidFunctionCode),
    }
}

// ─── Coil decoders ────────────────────────────────────────────────────────────

#[cfg(feature = "coils")]
fn decode_read_coils(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    // The task does no quantity validation here; client_core already knows what it requested.
    // We reconstruct Coils from packed bytes; use address=0 as placeholder — the caller
    // overwrites via its own context if it needs the real from_address.
    let bit_count = (bcp.byte_count as u16) * 8;
    let coils = Coils::new(0, bit_count)?.with_values(bcp.payload, bit_count)?;
    Ok(ClientResponse::Coils(coils))
}

#[cfg(feature = "coils")]
fn decode_write_single_coil(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let fields = pdu.write_single_u16_fields()?;
    let value = fields.value == 0xFF00;
    let mut coils = Coils::new(fields.address, 1)?;
    coils.set_value(fields.address, value)?;
    Ok(ClientResponse::Coils(coils))
}

#[cfg(feature = "coils")]
fn decode_write_multiple_coils(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let fields = pdu.read_window()?;
    let coils = Coils::new(fields.address, fields.quantity)?;
    Ok(ClientResponse::Coils(coils))
}

// ─── Discrete input decoder ───────────────────────────────────────────────────

#[cfg(feature = "discrete-inputs")]
fn decode_read_discrete_inputs(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    let bit_count = (bcp.byte_count as u16) * 8;
    let discrete_inputs = DiscreteInputs::new(0, bit_count)?.with_values(bcp.payload, bit_count)?;
    Ok(ClientResponse::DiscreteInputs(discrete_inputs))
}

// ─── Register decoders ────────────────────────────────────────────────────────

#[cfg(feature = "registers")]
fn decode_read_registers(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    if bcp.byte_count % 2 != 0 {
        return Err(MbusError::InvalidByteCount);
    }
    let quantity = bcp.byte_count as u16 / 2;
    let mut registers = Registers::new(0, quantity)?;
    for (i, chunk) in bcp.payload.chunks(2).enumerate() {
        if chunk.len() == 2 {
            let val = u16::from_be_bytes([chunk[0], chunk[1]]);
            registers.set_value(i as u16, val)?;
        }
    }
    Ok(ClientResponse::Registers(registers))
}

#[cfg(feature = "registers")]
fn decode_write_single_register(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let fields = pdu.write_single_u16_fields()?;
    Ok(ClientResponse::SingleRegisterWrite {
        address: fields.address,
        value: fields.value,
    })
}

#[cfg(feature = "registers")]
fn decode_write_multiple_registers(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let fields = pdu.read_window()?;
    let registers = Registers::new(fields.address, fields.quantity)?;
    Ok(ClientResponse::Registers(registers))
}

#[cfg(feature = "registers")]
fn decode_mask_write_register(_pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    Ok(ClientResponse::MaskWriteRegister)
}

// ─── FIFO queue decoder ───────────────────────────────────────────────────────

#[cfg(feature = "fifo")]
fn decode_read_fifo_queue(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let fp = pdu.fifo_payload()?;
    let fifo_count = fp.fifo_count as usize;
    let fifo_byte_count = fp.fifo_byte_count as usize;

    if fp.values.len() + 2 != fifo_byte_count {
        return Err(MbusError::InvalidAduLength);
    }
    if fifo_byte_count != 2 + fifo_count * 2 {
        return Err(MbusError::ParseError);
    }

    let mut values = [0u16; mbus_core::models::fifo_queue::MAX_FIFO_QUEUE_COUNT_PER_PDU];
    for (i, chunk) in fp.values.chunks_exact(2).enumerate() {
        if i >= values.len() {
            return Err(MbusError::BufferLenMissmatch);
        }
        values[i] = u16::from_be_bytes([chunk[0], chunk[1]]);
    }

    let fifo_queue = FifoQueue::new(0).with_values(values, fifo_count);
    Ok(ClientResponse::FifoQueue(fifo_queue))
}

// ─── File record decoders ─────────────────────────────────────────────────────

#[cfg(feature = "file-record")]
fn decode_read_file_record(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    let mut sub_requests: Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU> = Vec::new();
    let mut i = 0;

    while i < bcp.payload.len() {
        if i + 2 > bcp.payload.len() {
            return Err(MbusError::ParseError);
        }
        let file_resp_len = bcp.payload[i] as usize;
        let ref_type = bcp.payload[i + 1];

        if ref_type != FILE_RECORD_REF_TYPE {
            return Err(MbusError::ParseError);
        }
        if file_resp_len < 1 {
            return Err(MbusError::ParseError);
        }
        let data_len = file_resp_len - 1;
        if i + 1 + file_resp_len > bcp.payload.len() {
            return Err(MbusError::ParseError);
        }

        let raw_data = &bcp.payload[i + 2..i + 2 + data_len];
        if !raw_data.len().is_multiple_of(2) {
            return Err(MbusError::ParseError);
        }

        let mut record_data: Vec<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }> =
            Vec::new();
        for chunk in raw_data.chunks(2) {
            record_data
                .push(u16::from_be_bytes([chunk[0], chunk[1]]))
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        sub_requests
            .push(SubRequestParams {
                file_number: 0,
                record_number: 0,
                record_length: record_data.len() as u16,
                record_data: Some(record_data),
            })
            .map_err(|_| MbusError::BufferTooSmall)?;

        i += 1 + file_resp_len;
    }

    Ok(ClientResponse::FileRecordRead(sub_requests))
}

#[cfg(feature = "file-record")]
fn decode_write_file_record(_pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    Ok(ClientResponse::FileRecordWrite)
}

// ─── Diagnostics decoders ─────────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
fn decode_encapsulated_interface_transport(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let mtp = pdu.mei_type_payload()?;
    let mei_type_byte = mtp.mei_type_byte;

    // Read Device Identification path
    if mei_type_byte == EncapsulatedInterfaceType::ReadDeviceIdentification as u8 {
        let fields = pdu.read_device_id_fields()?;
        let read_device_id_code = ReadDeviceIdCode::try_from(fields.read_device_id_code_byte)?;
        let conformity_level = ConformityLevel::try_from(fields.conformity_level_byte)?;
        return Ok(ClientResponse::DeviceIdentification(
            DeviceIdentificationResponse {
                read_device_id_code,
                conformity_level,
                more_follows: fields.more_follows,
                next_object_id: ObjectId::from(fields.next_object_id_byte),
                objects_data: fields.objects_data,
                number_of_objects: fields.number_of_objects,
            },
        ));
    }

    // Generic encapsulated transport path
    let mei_type = EncapsulatedInterfaceType::try_from(mei_type_byte)?;
    let mut data: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
    if !mtp.payload.is_empty() {
        data.extend_from_slice(mtp.payload)
            .map_err(|_| MbusError::BufferTooSmall)?;
    }
    Ok(ClientResponse::EncapsulatedInterfaceTransport { mei_type, data })
}

#[cfg(feature = "diagnostics")]
fn decode_read_exception_status(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let status = pdu.single_byte_payload()?;
    Ok(ClientResponse::ExceptionStatus(status))
}

#[cfg(feature = "diagnostics")]
fn decode_diagnostics(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let sfp = pdu.sub_function_payload()?;
    let sub_function = DiagnosticSubFunction::try_from(sfp.sub_function)?;
    let mut data: Vec<u16, MAX_PDU_DATA_LEN> = Vec::new();
    for chunk in sfp.payload.chunks(2) {
        if chunk.len() == 2 {
            data.push(u16::from_be_bytes([chunk[0], chunk[1]]))
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }
    }
    Ok(ClientResponse::DiagnosticsData { sub_function, data })
}

#[cfg(feature = "diagnostics")]
fn decode_get_comm_event_counter(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let pair = pdu.u16_pair_fields()?;
    Ok(ClientResponse::CommEventCounter {
        status: pair.first,
        event_count: pair.second,
    })
}

#[cfg(feature = "diagnostics")]
fn decode_get_comm_event_log(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    if bcp.byte_count < 6 {
        return Err(MbusError::InvalidByteCount);
    }
    let p = bcp.payload;
    let status = u16::from_be_bytes([p[0], p[1]]);
    let event_count = u16::from_be_bytes([p[2], p[3]]);
    let message_count = u16::from_be_bytes([p[4], p[5]]);
    let mut events: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
    if p.len() > 6 {
        events
            .extend_from_slice(&p[6..])
            .map_err(|_| MbusError::BufferTooSmall)?;
    }
    Ok(ClientResponse::CommEventLog {
        status,
        event_count,
        message_count,
        events,
    })
}

#[cfg(feature = "diagnostics")]
fn decode_report_server_id(pdu: &Pdu) -> Result<ClientResponse, MbusError> {
    let bcp = pdu.byte_count_payload()?;
    let mut data: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
    if !bcp.payload.is_empty() {
        data.extend_from_slice(bcp.payload)
            .map_err(|_| MbusError::BufferTooSmall)?;
    }
    Ok(ClientResponse::ReportServerId(data))
}
