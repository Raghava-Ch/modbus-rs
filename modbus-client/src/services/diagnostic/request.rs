//! Modbus Diagnostics and Encapsulated Interface Transport Service Module
//!
//! This module provides structures and logic for handling Modbus diagnostic functions
//! and Encapsulated Interface Transport (MEI) operations.
//!
//! Key functionalities include:
//! - **Read Device Identification (FC 43 / MEI 0x0E)**: Retrieving server identity (Vendor, Product Code, etc.).
//! - **Serial Line Diagnostics**: Support for FC 0x07 (Exception Status), FC 0x08 (Diagnostics),
//!   FC 0x0B (Comm Event Counter), FC 0x0C (Comm Event Log), and FC 0x11 (Report Server ID).
//! - **Encapsulated Interface Transport (FC 43)**: Generic tunneling for MEI types like CANopen.
//!
//! This module is designed for `no_std` environments using `heapless` collections.

use crate::services::diagnostic::{ObjectId, ReadDeviceIdCode};
use mbus_core::{
    data_unit::common::{MAX_PDU_DATA_LEN, Pdu},
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode},
};
use heapless::Vec;

/// Service for handling Modbus Diagnostics function codes.
pub(super) struct ReqPduCompiler {}

impl ReqPduCompiler {
    /// Creates a Read Exception Status (FC 0x07) request PDU.
    ///
    /// This function code is used to read the contents of eight Exception Status outputs in a remote device.
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub(super) fn read_exception_status_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::ReadExceptionStatus, Vec::new(), 0))
    }

    /// Creates a Diagnostics (FC 0x08) request PDU.
    ///
    /// # Arguments
    /// * `sub_function` - The sub-function code.
    /// * `data` - The data to be sent (16-bit words).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub(super) fn diagnostics_request(
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) -> Result<Pdu, MbusError> {
        // Max data length in bytes = 252.
        // Sub-function takes 2 bytes.
        // Remaining for data = 250 bytes = 125 words.
        if data.len() > 125 {
            return Err(MbusError::InvalidPduLength);
        }

        let mut pdu_data: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        pdu_data
            .extend_from_slice(&sub_function.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for word in data {
            pdu_data
                .extend_from_slice(&word.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        Ok(Pdu::new(
            FunctionCode::Diagnostics,
            pdu_data,
            (2 + data.len() * 2) as u8,
        ))
    }

    /// Creates a Get Comm Event Counter (FC 0x0B) request PDU.
    pub(super) fn get_comm_event_counter_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::GetCommEventCounter, Vec::new(), 0))
    }

    /// Creates a Get Comm Event Log (FC 0x0C) request PDU.
    pub(super) fn get_comm_event_log_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::GetCommEventLog, Vec::new(), 0))
    }

    /// Creates a Report Server ID (FC 0x11) request PDU.
    pub(super) fn report_server_id_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::ReportServerId, Vec::new(), 0))
    }

    /// Creates an Encapsulated Interface Transport (FC 0x2B) request PDU.
    pub(super) fn encapsulated_interface_transport_request(
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) -> Result<Pdu, MbusError> {
        let mut pdu_data = Vec::new();
        pdu_data
            .push(mei_type.into())
            .map_err(|_| MbusError::BufferTooSmall)?;
        pdu_data
            .extend_from_slice(data)
            .map_err(|_| MbusError::BufferTooSmall)?;

        Ok(Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            pdu_data,
            (1 + data.len()) as u8,
        ))
    }

    /// Creates a Read Device Identification (FC 0x2B / MEI 0x0E) request PDU.
    ///
    /// # Arguments
    /// * `read_device_id_code` - The code defining the type of access (01, 02, 03, 04).
    /// * `object_id` - The object ID to start reading from (0x00 - 0xFF).
    pub(super) fn read_device_identification_request(
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
    ) -> Result<Pdu, MbusError> {
        let mut data = Vec::new();
        data.push(EncapsulatedInterfaceType::ReadDeviceIdentification as u8)
            .map_err(|_| MbusError::BufferTooSmall)?;
        data.push(read_device_id_code as u8)
            .map_err(|_| MbusError::BufferTooSmall)?;
        data.push(object_id.into())
            .map_err(|_| MbusError::BufferTooSmall)?;

        Ok(Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            data,
            3,
        ))
    }
}
