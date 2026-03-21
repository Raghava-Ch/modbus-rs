use heapless::Vec;

use crate::{
    app::DiagnosticsResponse,
    services::diagnostic::{
        ConformityLevel, DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode,
    },
    services::{ClientCommon, ClientServices, ExpectedResponse},
};
use mbus_core::{
    data_unit::common::{MAX_PDU_DATA_LEN, ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode},
    transport::Transport,
};

pub(super) struct ResponseParser;

impl ResponseParser {
    // --- Parsing Methods ---
    /// Parses a Get Comm Event Log (FC 0x0C) response PDU.
    /// Returns (Status, Event Count, Message Count, Events).
    pub(super) fn parse_get_comm_event_log_response(
        pdu: &Pdu,
    ) -> Result<(u16, u16, u16, Vec<u8, MAX_PDU_DATA_LEN>), MbusError> {
        if pdu.function_code() != FunctionCode::GetCommEventLog {
            return Err(MbusError::InvalidFunctionCode);
        }
        let data = pdu.data().as_slice();
        if data.len() < 7 {
            return Err(MbusError::InvalidDataLen);
        }
        let byte_count = data[0] as usize;
        // Byte count includes: Status(2) + EventCount(2) + MsgCount(2) + Events(N)
        // So N = byte_count - 6
        if byte_count < 6 {
            return Err(MbusError::InvalidByteCount);
        }

        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidDataLen);
        }

        let status = u16::from_be_bytes([data[1], data[2]]);
        let event_count = u16::from_be_bytes([data[3], data[4]]);
        let message_count = u16::from_be_bytes([data[5], data[6]]);

        let mut events = Vec::new();
        if data.len() > 7 {
            events
                .extend_from_slice(&data[7..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok((status, event_count, message_count, events))
    }

    /// Parses a Report Server ID (FC 0x11) response PDU.
    /// Returns the raw data (Server ID + Run Indicator + Additional Data).
    pub(super) fn parse_report_server_id_response(
        pdu: &Pdu,
    ) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError> {
        if pdu.function_code() != FunctionCode::ReportServerId {
            return Err(MbusError::InvalidFunctionCode);
        }
        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }
        let byte_count = data[0] as usize;
        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidByteCount);
        }

        let mut server_data = Vec::new();
        if data.len() > 1 {
            server_data
                .extend_from_slice(&data[1..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok(server_data)
    }

    /// Parses an Encapsulated Interface Transport (FC 0x2B) response PDU.
    pub(super) fn parse_encapsulated_interface_transport_response(
        pdu: &Pdu,
    ) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError> {
        if pdu.function_code() != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }

        let mei_type = EncapsulatedInterfaceType::try_from(data[0])?;
        if EncapsulatedInterfaceType::CanopenGeneralReference != mei_type {
            return Err(MbusError::InvalidMeiType);
        }

        let mut response_data = Vec::new();
        if data.len() > 1 {
            response_data
                .extend_from_slice(&data[1..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok(response_data)
    }

    /// Parses a Read Device Identification (FC 0x2B / MEI 0x0E) response PDU.
    pub(super) fn parse_read_device_identification_response(
        device_id_code: ReadDeviceIdCode,
        pdu: &Pdu,
    ) -> Result<DeviceIdentificationResponse, MbusError> {
        if pdu.function_code() != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        // Min length: MEI(1) + ReadCode(1) + Conf(1) + More(1) + NextId(1) + NumObj(1) = 6
        if data.len() < 6 {
            return Err(MbusError::InvalidDataLen);
        }

        if data[0] != EncapsulatedInterfaceType::ReadDeviceIdentification as u8 {
            return Err(MbusError::InvalidMeiType);
        }

        let read_device_id_code = ReadDeviceIdCode::try_from(data[1])?;
        let conformity_level = ConformityLevel::try_from(data[2])?;
        let more_follows = data[3];
        let next_object_id = ObjectId::from(data[4]);
        let number_of_objects = data[5];

        if read_device_id_code != device_id_code {
            return Err(MbusError::InvalidDeviceIdentification);
        }

        // Validate the data integrity before storing it
        let mut offset = 6;

        for _ in 0..number_of_objects as usize {
            if offset + 2 > data.len() {
                return Err(MbusError::InvalidPduLength);
            }
            let _obj_id = ObjectId::from(data[offset]);
            let obj_len = data[offset + 1] as usize;
            offset += 2;

            if offset + obj_len > data.len() {
                return Err(MbusError::InvalidPduLength);
            }
            offset += obj_len;
        }

        // Store the raw objects data (everything after the 6-byte header)
        let mut objects_data = Vec::new();
        if data.len() > 6 {
            objects_data
                .extend_from_slice(&data[6..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok(DeviceIdentificationResponse {
            read_device_id_code,
            conformity_level,
            more_follows: more_follows == 0xFF,
            next_object_id,
            objects_data,
            number_of_objects,
        })
    }

    /// Parses a Read Exception Status (FC 0x07) response PDU.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    ///
    /// # Returns
    /// A `Result` containing the status byte, or an `MbusError`.
    pub(super) fn parse_read_exception_status_response(pdu: &Pdu) -> Result<u8, MbusError> {
        if pdu.function_code() != FunctionCode::ReadExceptionStatus {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.len() != 1 {
            return Err(MbusError::InvalidDataLen);
        }

        Ok(data[0])
    }

    /// Parses a Diagnostics (FC 0x08) response PDU.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu`.
    ///
    /// # Returns
    /// A `Result` containing the sub-function code and the data words.
    pub(super) fn parse_diagnostics_response(pdu: &Pdu) -> Result<(u16, Vec<u16, 125>), MbusError> {
        if pdu.function_code() != FunctionCode::Diagnostics {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.len() < 2 {
            return Err(MbusError::InvalidDataLen);
        }
        if data.len() % 2 != 0 {
            return Err(MbusError::InvalidDataLen);
        }

        let sub_function = u16::from_be_bytes([data[0], data[1]]);

        let mut values = Vec::new();
        for chunk in data[2..].chunks(2) {
            let val = u16::from_be_bytes([chunk[0], chunk[1]]);
            values
                .push(val)
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        Ok((sub_function, values))
    }

    /// Parses a Get Comm Event Counter (FC 0x0B) response PDU.
    /// Returns (Status, Event Count).
    pub(super) fn parse_get_comm_event_counter_response(
        pdu: &Pdu,
    ) -> Result<(u16, u16), MbusError> {
        if pdu.function_code() != FunctionCode::GetCommEventCounter {
            return Err(MbusError::InvalidFunctionCode);
        }
        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidDataLen);
        }
        let status = u16::from_be_bytes([data[0], data[1]]);
        let event_count = u16::from_be_bytes([data[2], data[3]]);
        Ok((status, event_count))
    }
}

// --- Response Handlers ---
impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + DiagnosticsResponse,
{
    /// Handles a Read Device Identification response.
    pub(super) fn handle_read_device_identification_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let device_id_code = ctx.operation_meta.device_id_code();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_read_device_identification_response(device_id_code, pdu) {
            Ok(response) => {
                self.app.read_device_identification_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    &response,
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }

    /// Handles a generic Encapsulated Interface Transport response (FC 43 / 0x2B).
    ///
    /// Parses the response PDU to extract the MEI type and the associated data.
    /// Returns the MEI type and the raw data payload.
    pub(super) fn handle_encapsulated_interface_transport_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_encapsulated_interface_transport_response(pdu) {
            Ok(response) => {
                self.app.encapsulated_interface_transport_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    EncapsulatedInterfaceType::CanopenGeneralReference,
                    response.as_slice(),
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }

    /// Handles a Read Exception Status response (FC 0x07). Serial Line only.
    pub(super) fn handle_read_exception_status_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_read_exception_status_response(pdu) {
            Ok(response) => {
                self.app.read_exception_status_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    response,
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }

    /// Handles a Diagnostics response (FC 0x08). Serial Line only.
    pub(super) fn handle_diagnostics_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let response = match ResponseParser::parse_diagnostics_response(pdu) {
            Ok(response) => response,
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
                return;
            }
        };
        let sub_function = match DiagnosticSubFunction::try_from(response.0) {
            Ok(sub_function) => sub_function,
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
                return;
            }
        };
        self.app.diagnostics_response(
            transaction_id,
            unit_id_or_slave_addr,
            sub_function,
            response.1.as_slice(),
        );
    }

    /// Handles a Get Comm Event Counter response (FC 0x0B). Serial Line only.
    pub(super) fn handle_get_comm_event_counter_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_get_comm_event_counter_response(pdu) {
            Ok(response) => {
                self.app.get_comm_event_counter_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    response.0,
                    response.1,
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }

    /// Handles a Get Comm Event Log response (FC 0x0C). Serial Line only.
    pub(super) fn handle_get_comm_event_log_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_get_comm_event_log_response(pdu) {
            Ok(response) => {
                self.app.get_comm_event_log_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    response.0,
                    response.1,
                    response.2,
                    response.3.as_slice(),
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }

    /// Handles a Report Server ID response (FC 0x11). Serial Line only.
    pub(super) fn handle_report_server_id_rsp(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        match ResponseParser::parse_report_server_id_response(pdu) {
            Ok(response) => {
                self.app.report_server_id_response(
                    transaction_id,
                    unit_id_or_slave_addr,
                    response.as_slice(),
                );
            }
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
            }
        }
    }
}
