use heapless::Vec;

use crate::services::diagnostic::{ObjectId, ReadDeviceIdCode, request::ReqPduCompiler};

use mbus_core::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN},
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    transport::TransportType,
};

/// Service for handling Modbus Diagnostics function codes.
#[derive(Debug, Clone)]
pub struct ServiceBuilder;

impl ServiceBuilder {
    /// Sends a Read Device Identification request.
    pub fn read_device_identification(
        txn_id: u16,
        unit_id: u8,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu =
            ReqPduCompiler::read_device_identification_request(read_device_id_code, object_id)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a generic Encapsulated Interface Transport request (FC 43 / 0x2B).
    ///
    /// This function allows sending requests for specific MEI types, such as CANopen General Reference (0x0D).
    /// The `data` payload is appended after the MEI type in the PDU.
    pub fn encapsulated_interface_transport(
        txn_id: u16,
        unit_id: u8,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::encapsulated_interface_transport_request(mei_type, data)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Read Exception Status request (FC 0x07). Serial Line only.
    pub fn read_exception_status(
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        Self::check_serial(&transport_type)?;
        let pdu = ReqPduCompiler::read_exception_status_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Diagnostics request (FC 0x08). Serial Line only.
    pub fn diagnostics(
        unit_id: u8,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        Self::check_serial(&transport_type)?;
        let pdu = ReqPduCompiler::diagnostics_request(sub_function, data)?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Get Comm Event Counter request (FC 0x0B). Serial Line only.
    pub fn get_comm_event_counter(
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        Self::check_serial(&transport_type)?;
        let pdu = ReqPduCompiler::get_comm_event_counter_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Get Comm Event Log request (FC 0x0C). Serial Line only.
    pub fn get_comm_event_log(
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        Self::check_serial(&transport_type)?;
        let pdu = ReqPduCompiler::get_comm_event_log_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Report Server ID request (FC 0x11). Serial Line only.
    pub fn report_server_id(
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        Self::check_serial(&transport_type)?;
        let pdu = ReqPduCompiler::report_server_id_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    // --- Helpers ---

    /// Checks if the transport type is serial.
    fn check_serial(transport_type: &TransportType) -> Result<(), MbusError> {
        match transport_type {
            TransportType::StdSerial(_) | TransportType::CustomSerial(_) => Ok(()),
            _ => Err(MbusError::InvalidTransport), // Feature not supported on non-serial transport
        }
    }
}
