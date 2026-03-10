use crate::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN, Pdu},
    device_identification::{ConformityLevel, ObjectId, ReadDeviceIdCode},
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode},
    transport::TransportType,
};
use heapless::Vec;

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdObject {
    pub object_id: ObjectId,
    pub value: Vec<u8, MAX_PDU_DATA_LEN>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdentificationResponse {
    pub read_device_id_code: ReadDeviceIdCode,
    pub conformity_level: ConformityLevel,
    pub more_follows: bool,
    pub next_object_id: ObjectId,
    pub objects_data: Vec<u8, MAX_PDU_DATA_LEN>,
    pub number_of_objects: u8,
}

#[derive(Debug, Clone)]
pub struct DiagnosticsService;

impl DiagnosticsService {
    pub fn new() -> Self {
        Self
    }

    /// Sends a Read Device Identification request.
    pub fn read_device_identification(
        &self,
        txn_id: u16,
        unit_id: u8,
        read_device_id_code: ReadDeviceIdCode,
        object_id: ObjectId,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu =
            DiagnosticsReqPdu::read_device_identification_request(read_device_id_code, object_id)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a generic Encapsulated Interface Transport request (FC 43 / 0x2B).
    ///
    /// This function allows sending requests for specific MEI types, such as CANopen General Reference (0x0D).
    /// The `data` payload is appended after the MEI type in the PDU.
    pub fn encapsulated_interface_transport(
        &self,
        txn_id: u16,
        unit_id: u8,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = DiagnosticsReqPdu::encapsulated_interface_transport_request(mei_type, data)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Handles a Read Device Identification response.
    pub fn handle_read_device_identification_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<DeviceIdentificationResponse, MbusError> {
        if function_code != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_read_device_identification_response(pdu)
    }

    /// Handles a generic Encapsulated Interface Transport response (FC 43 / 0x2B).
    ///
    /// Parses the response PDU to extract the MEI type and the associated data.
    /// Returns the MEI type and the raw data payload.
    pub fn handle_encapsulated_interface_transport_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(EncapsulatedInterfaceType, Vec<u8, MAX_PDU_DATA_LEN>), MbusError> {
        if function_code != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_encapsulated_interface_transport_response(pdu)
    }

    /// Sends a Read Exception Status request (FC 0x07). Serial Line only.
    pub fn read_exception_status(
        &self,
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.check_serial(&transport_type)?;
        let pdu = DiagnosticsReqPdu::read_exception_status_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Diagnostics request (FC 0x08). Serial Line only.
    pub fn diagnostics(
        &self,
        unit_id: u8,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.check_serial(&transport_type)?;
        let pdu = DiagnosticsReqPdu::diagnostics_request(sub_function, data)?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Get Comm Event Counter request (FC 0x0B). Serial Line only.
    pub fn get_comm_event_counter(
        &self,
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.check_serial(&transport_type)?;
        let pdu = DiagnosticsReqPdu::get_comm_event_counter_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Get Comm Event Log request (FC 0x0C). Serial Line only.
    pub fn get_comm_event_log(
        &self,
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.check_serial(&transport_type)?;
        let pdu = DiagnosticsReqPdu::get_comm_event_log_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    /// Sends a Report Server ID request (FC 0x11). Serial Line only.
    pub fn report_server_id(
        &self,
        unit_id: u8,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        self.check_serial(&transport_type)?;
        let pdu = DiagnosticsReqPdu::report_server_id_request()?;
        // txn_id is dont care in this serial transport case
        common::compile_adu_frame(0, unit_id, pdu, transport_type)
    }

    // --- Response Handlers ---

    pub fn handle_read_exception_status_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<u8, MbusError> {
        if function_code != FunctionCode::ReadExceptionStatus {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_read_exception_status_response(pdu)
    }

    pub fn handle_diagnostics_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(u16, Vec<u16, 125>), MbusError> {
        if function_code != FunctionCode::Diagnostics {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_diagnostics_response(pdu)
    }

    pub fn handle_get_comm_event_counter_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(u16, u16), MbusError> {
        if function_code != FunctionCode::GetCommEventCounter {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_get_comm_event_counter_response(pdu)
    }

    pub fn handle_get_comm_event_log_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(u16, u16, u16, Vec<u8, MAX_PDU_DATA_LEN>), MbusError> {
        if function_code != FunctionCode::GetCommEventLog {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_get_comm_event_log_response(pdu)
    }

    pub fn handle_report_server_id_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError> {
        if function_code != FunctionCode::ReportServerId {
            return Err(MbusError::InvalidFunctionCode);
        }
        DiagnosticsReqPdu::parse_report_server_id_response(pdu)
    }

    // --- Helpers ---

    fn check_serial(&self, transport_type: &TransportType) -> Result<(), MbusError> {
        match transport_type {
            TransportType::StdSerial(_, _) | TransportType::CustomSerial(_, _) => Ok(()),
            _ => Err(MbusError::InvalidTransport), // Feature not supported on non-serial transport
        }
    }
}

impl DeviceIdentificationResponse {
    /// Returns an iterator over the device identification objects.
    pub fn objects(&self) -> DeviceIdObjectIterator<'_> {
        DeviceIdObjectIterator {
            data: &self.objects_data,
            offset: 0,
            count: 0,
            total: self.number_of_objects,
        }
    }
}

pub struct DeviceIdObjectIterator<'a> {
    data: &'a [u8],
    offset: usize,
    count: u8,
    total: u8,
}

impl<'a> Iterator for DeviceIdObjectIterator<'a> {
    type Item = Result<DeviceIdObject, MbusError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.count >= self.total {
            return None;
        }

        // Parsing logic is handled internally in the iterator step
        // We reuse the parsing logic from the original implementation but applied incrementally
        self.parse_next()
    }
}

pub struct DiagnosticsReqPdu {}

impl DiagnosticsReqPdu {
    /// Creates a Read Exception Status (FC 0x07) request PDU.
    ///
    /// This function code is used to read the contents of eight Exception Status outputs in a remote device.
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub fn read_exception_status_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::ReadExceptionStatus, Vec::new(), 0))
    }

    /// Parses a Read Exception Status (FC 0x07) response PDU.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    ///
    /// # Returns
    /// A `Result` containing the status byte, or an `MbusError`.
    pub fn parse_read_exception_status_response(pdu: &Pdu) -> Result<u8, MbusError> {
        if pdu.function_code() != FunctionCode::ReadExceptionStatus {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 1 {
            return Err(MbusError::InvalidPduLength);
        }

        Ok(data[0])
    }

    /// Creates a Diagnostics (FC 0x08) request PDU.
    ///
    /// # Arguments
    /// * `sub_function` - The sub-function code.
    /// * `data` - The data to be sent (16-bit words).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError`.
    pub fn diagnostics_request(
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

    /// Parses a Diagnostics (FC 0x08) response PDU.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu`.
    ///
    /// # Returns
    /// A `Result` containing the sub-function code and the data words.
    pub fn parse_diagnostics_response(pdu: &Pdu) -> Result<(u16, Vec<u16, 125>), MbusError> {
        if pdu.function_code() != FunctionCode::Diagnostics {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() < 2 {
            return Err(MbusError::InvalidPduLength);
        }
        if data.len() % 2 != 0 {
            return Err(MbusError::ParseError);
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

    /// Creates a Get Comm Event Counter (FC 0x0B) request PDU.
    pub fn get_comm_event_counter_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::GetCommEventCounter, Vec::new(), 0))
    }

    /// Parses a Get Comm Event Counter (FC 0x0B) response PDU.
    /// Returns (Status, Event Count).
    pub fn parse_get_comm_event_counter_response(pdu: &Pdu) -> Result<(u16, u16), MbusError> {
        if pdu.function_code() != FunctionCode::GetCommEventCounter {
            return Err(MbusError::ParseError);
        }
        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidPduLength);
        }
        let status = u16::from_be_bytes([data[0], data[1]]);
        let event_count = u16::from_be_bytes([data[2], data[3]]);
        Ok((status, event_count))
    }

    /// Creates a Get Comm Event Log (FC 0x0C) request PDU.
    pub fn get_comm_event_log_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::GetCommEventLog, Vec::new(), 0))
    }

    /// Parses a Get Comm Event Log (FC 0x0C) response PDU.
    /// Returns (Status, Event Count, Message Count, Events).
    pub fn parse_get_comm_event_log_response(
        pdu: &Pdu,
    ) -> Result<(u16, u16, u16, Vec<u8, MAX_PDU_DATA_LEN>), MbusError> {
        if pdu.function_code() != FunctionCode::GetCommEventLog {
            return Err(MbusError::ParseError);
        }
        let data = pdu.data().as_slice();
        if data.len() < 7 {
            return Err(MbusError::InvalidPduLength);
        }
        let byte_count = data[0] as usize;
        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidPduLength);
        }
        // Byte count includes: Status(2) + EventCount(2) + MsgCount(2) + Events(N)
        // So N = byte_count - 6
        if byte_count < 6 {
            return Err(MbusError::ParseError);
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

    /// Creates a Report Server ID (FC 0x11) request PDU.
    pub fn report_server_id_request() -> Result<Pdu, MbusError> {
        Ok(Pdu::new(FunctionCode::ReportServerId, Vec::new(), 0))
    }

    /// Parses a Report Server ID (FC 0x11) response PDU.
    /// Returns the raw data (Server ID + Run Indicator + Additional Data).
    pub fn parse_report_server_id_response(
        pdu: &Pdu,
    ) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError> {
        if pdu.function_code() != FunctionCode::ReportServerId {
            return Err(MbusError::ParseError);
        }
        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }
        let byte_count = data[0] as usize;
        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidPduLength);
        }

        let mut server_data = Vec::new();
        if data.len() > 1 {
            server_data
                .extend_from_slice(&data[1..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok(server_data)
    }

    /// Creates an Encapsulated Interface Transport (FC 0x2B) request PDU.
    pub fn encapsulated_interface_transport_request(
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

    /// Parses an Encapsulated Interface Transport (FC 0x2B) response PDU.
    pub fn parse_encapsulated_interface_transport_response(
        pdu: &Pdu,
    ) -> Result<(EncapsulatedInterfaceType, Vec<u8, MAX_PDU_DATA_LEN>), MbusError> {
        if pdu.function_code() != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let mei_type = EncapsulatedInterfaceType::try_from(data[0])?;
        let mut response_data = Vec::new();
        if data.len() > 1 {
            response_data
                .extend_from_slice(&data[1..])
                .map_err(|_| MbusError::BufferTooSmall)?;
        }

        Ok((mei_type, response_data))
    }

    /// Creates a Read Device Identification (FC 0x2B / MEI 0x0E) request PDU.
    ///
    /// # Arguments
    /// * `read_device_id_code` - The code defining the type of access (01, 02, 03, 04).
    /// * `object_id` - The object ID to start reading from (0x00 - 0xFF).
    pub fn read_device_identification_request(
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

    /// Parses a Read Device Identification (FC 0x2B / MEI 0x0E) response PDU.
    pub fn parse_read_device_identification_response(
        pdu: &Pdu,
    ) -> Result<DeviceIdentificationResponse, MbusError> {
        if pdu.function_code() != FunctionCode::EncapsulatedInterfaceTransport {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        // Min length: MEI(1) + ReadCode(1) + Conf(1) + More(1) + NextId(1) + NumObj(1) = 6
        if data.len() < 6 {
            return Err(MbusError::InvalidPduLength);
        }

        if data[0] != EncapsulatedInterfaceType::ReadDeviceIdentification as u8 {
            return Err(MbusError::ParseError);
        }

        let read_device_id_code = ReadDeviceIdCode::try_from(data[1])?;
        let conformity_level = ConformityLevel::try_from(data[2])?;
        let more_follows = data[3];
        let next_object_id = ObjectId::from(data[4]);
        let number_of_objects = data[5];

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
}

impl<'a> DeviceIdObjectIterator<'a> {
    /// Parses the next `DeviceIdObject` from the raw objects data buffer.
    ///
    /// Each object in the stream consists of:
    /// - Object Id (1 byte)
    /// - Object Length (1 byte)
    /// - Object Value (N bytes)
    fn parse_next(&mut self) -> Option<Result<DeviceIdObject, MbusError>> {
        // Check if there is enough data for the 2-byte header (Id + Length)
        if self.offset + 2 > self.data.len() {
            return Some(Err(MbusError::InvalidPduLength));
        }
        let obj_id = ObjectId::from(self.data[self.offset]);
        let obj_len = self.data[self.offset + 1] as usize;
        self.offset += 2; // Move past the header

        // Ensure the remaining data contains the full object value
        if self.offset + obj_len > self.data.len() {
            return Some(Err(MbusError::InvalidPduLength));
        }

        let mut value = Vec::new();
        // Copy the object value into the heapless::Vec
        if value
            .extend_from_slice(&self.data[self.offset..self.offset + obj_len])
            .is_err()
        {
            return Some(Err(MbusError::BufferTooSmall));
        }

        self.offset += obj_len;
        self.count += 1;

        Some(Ok(DeviceIdObject {
            object_id: obj_id,
            value,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_exception_status_request() {
        let pdu = DiagnosticsReqPdu::read_exception_status_request().unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadExceptionStatus);
        assert_eq!(pdu.data_len(), 0);
    }

    #[test]
    fn test_diagnostics_request() {
        let sub_function = DiagnosticSubFunction::ReturnQueryData; // Return Query Data
        let data = [0xA5A5, 0x5A5A];
        let pdu = DiagnosticsReqPdu::diagnostics_request(sub_function, &data).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::Diagnostics);
        assert_eq!(pdu.data_len(), 6); // 2 sub-func + 4 data
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0xA5, 0xA5, 0x5A, 0x5A]);
    }

    #[test]
    fn test_parse_diagnostics_response() {
        let data = [0x00, 0x00, 0xA5, 0xA5, 0x5A, 0x5A];
        let pdu = Pdu::new(
            FunctionCode::Diagnostics,
            Vec::from_slice(&data).unwrap(),
            6,
        );
        let (sub_func, values) = DiagnosticsReqPdu::parse_diagnostics_response(&pdu).unwrap();
        assert_eq!(sub_func, 0x0000);
        assert_eq!(values.as_slice(), &[0xA5A5, 0x5A5A]);
    }

    #[test]
    fn test_read_device_identification_request() {
        let pdu = DiagnosticsReqPdu::read_device_identification_request(
            ReadDeviceIdCode::Basic,
            ObjectId::from(0x00),
        )
        .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::EncapsulatedInterfaceTransport
        );
        assert_eq!(pdu.data().as_slice(), &[0x0E, 0x01, 0x00]);
    }

    #[test]
    fn test_parse_read_device_identification_response() {
        // MEI(0E), Code(01), Conf(81), More(00), Next(00), Num(02)
        // Obj1: Id(00), Len(03), Val("Foo")
        // Obj2: Id(01), Len(03), Val("Bar")
        let data = [
            0x0E, 0x01, 0x81, 0x00, 0x00, 0x02, 0x00, 0x03, 0x46, 0x6F, 0x6F, 0x01, 0x03, 0x42,
            0x61, 0x72,
        ];
        let pdu = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data).unwrap(),
            data.len() as u8,
        );
        let resp = DiagnosticsReqPdu::parse_read_device_identification_response(&pdu).unwrap();

        assert_eq!(resp.read_device_id_code, ReadDeviceIdCode::Basic);
        assert_eq!(
            resp.conformity_level,
            ConformityLevel::BasicStreamAndIndividual
        );
        assert_eq!(resp.more_follows, false);
        assert_eq!(resp.next_object_id, ObjectId::from(0x00));

        let objects: Vec<DeviceIdObject, 10> = resp.objects().map(|r| r.unwrap()).collect();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].object_id, ObjectId::from(0x00));
        assert_eq!(objects[0].value.as_slice(), b"Foo");
        assert_eq!(objects[1].object_id, ObjectId::from(0x01));
        assert_eq!(objects[1].value.as_slice(), b"Bar");
    }

    #[test]
    fn test_parse_read_device_identification_response_malformed() {
        // Case 1: Truncated header (missing Number of Objects)
        let data_short = [0x0E, 0x01, 0x81, 0x00, 0x00];
        let pdu_short = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data_short).unwrap(),
            5,
        );
        assert_eq!(
            DiagnosticsReqPdu::parse_read_device_identification_response(&pdu_short).unwrap_err(),
            MbusError::InvalidPduLength
        );

        // Case 2: Object length exceeds available data
        // Num Objects = 1. Obj Id = 00. Obj Len = 05. But only 3 bytes ("Foo") follow.
        let data_overflow = [
            0x0E, 0x01, 0x81, 0x00, 0x00, 0x01, 0x00, 0x05, 0x46, 0x6F, 0x6F,
        ];
        let pdu_overflow = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data_overflow).unwrap(),
            data_overflow.len() as u8,
        );
        assert_eq!(
            DiagnosticsReqPdu::parse_read_device_identification_response(&pdu_overflow)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    #[test]
    fn test_encapsulated_interface_transport_request() {
        let mei_type = EncapsulatedInterfaceType::CanopenGeneralReference;
        let data = [0x01, 0x02, 0x03];
        let pdu =
            DiagnosticsReqPdu::encapsulated_interface_transport_request(mei_type, &data).unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::EncapsulatedInterfaceTransport
        );
        assert_eq!(pdu.data().as_slice(), &[0x0D, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_parse_encapsulated_interface_transport_response() {
        let data = [0x0D, 0x01, 0x02, 0x03];
        let pdu = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data).unwrap(),
            4,
        );
        let (mei, resp_data) =
            DiagnosticsReqPdu::parse_encapsulated_interface_transport_response(&pdu).unwrap();
        assert_eq!(mei, EncapsulatedInterfaceType::CanopenGeneralReference);
        assert_eq!(resp_data.as_slice(), &[0x01, 0x02, 0x03]);
    }
}
