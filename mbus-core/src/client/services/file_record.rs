use heapless::Vec;

use crate::{
    data_unit::common::{AdditionalAddress, MAX_ADU_FRAME_LEN, MbapHeader, ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::{FunctionCode, MAX_PDU_DATA_LEN},
    transport::TransportType,
};

/// Maximum number of sub-requests allowed in a single PDU (35).
pub const MAX_SUB_REQUESTS_PER_PDU: usize = 35;
/// Byte count is 1 byte for each sub-request 
///(reference type + file number + record number + record length) + 1 byte for the byte count itself
const SUB_REQ_PARAM_BYTE_LEN: usize = 6 + 1; 
/// The reference type for file record requests (0x06).
const FILE_RECORD_REF_TYPE: u8 = 0x06;

trait PduDataBytes {
    /// Converts the sub-request parameters into a byte vector for the PDU.
    fn to_sub_req_pdu_bytes(&self) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError>;
}

/// Parameters for a single file record sub-request.
#[derive(Debug, Clone, PartialEq)]
pub struct SubRequestParams {
    /// The file number to be read/written.
    pub file_number: u16,
    /// The starting record number.
    pub record_number: u16,
    /// The length of the record (number of registers).
    pub record_length: u16,
    /// The data to be written (only for write requests).
    pub record_data: Option<Vec<u16, MAX_PDU_DATA_LEN>>, // Only used for write requests, None for read requests
}
/// Represents a collection of sub-requests for File Record operations.
pub struct SubRequest {
    params: Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, // maximum of 35 sub-requests per PDU
    total_read_bytes_length: u16,
}

impl SubRequest {
    /// Creates a new empty `SubRequest`.
    pub fn new() -> Self {
        SubRequest {
            params: Vec::new(),
            total_read_bytes_length: 0,
        }
    }

    /// Adds a sub-request for reading a file record.
    ///
    /// # Arguments
    /// * `file_number` - The file number.
    /// * `record_number` - The starting record number.
    /// * `record_length` - The number of registers to read.
    pub fn add_read_sub_request(
        &mut self,
        file_number: u16,
        record_number: u16,
        record_length: u16,
    ) -> Result<(), MbusError> {
        if self.params.len() >= MAX_SUB_REQUESTS_PER_PDU {
            return Err(MbusError::TooManyFileReadSubRequests);
        }
        // Calculate expected response size to prevent overflow
        // Response PDU: FC(1) + ByteCount(1) + N * (Len(1) + Ref(1) + Data(Regs*2))
        // Total bytes = 2 + 2*N + 2*TotalRegs <= 253
        // N + TotalRegs <= 125
        // 125 is the approximate limit for (SubRequests + TotalRegisters) to fit in 253 bytes.
        if (self.params.len() as u16 + 1) + (self.total_read_bytes_length + record_length) > 125 {
            return Err(MbusError::FileReadPduOverflow);
        }
        self.params
            .push(SubRequestParams {
                file_number,
                record_number,
                record_length,
                record_data: None,
            })
            .map_err(|_| MbusError::TooManyFileReadSubRequests)?;

        self.total_read_bytes_length += record_length;
        Ok(())
    }

    /// Adds a sub-request for writing a file record.
    ///
    /// # Arguments
    /// * `file_number` - The file number.
    /// * `record_number` - The starting record number.
    /// * `record_length` - The number of registers to write.
    /// * `record_data` - The data to write.
    pub fn add_write_sub_request(
        &mut self,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: Vec<u16, MAX_PDU_DATA_LEN>,
    ) -> Result<(), MbusError> {
        if self.params.len() >= MAX_SUB_REQUESTS_PER_PDU {
            return Err(MbusError::TooManyFileReadSubRequests);
        }
        if record_data.len() != record_length as usize {
            return Err(MbusError::BufferLenMissmatch);
        }

        // Calculate projected PDU size: 1 (Byte Count Field) + Current Payload + New SubReq (7 + Data)
        let current_payload_size = self.byte_count();
        // 7 bytes header (Ref + File + RecNum + RecLen) + Data bytes (2 * registers)
        let new_sub_req_size = SUB_REQ_PARAM_BYTE_LEN + (record_data.len() * 2);

        // Check if adding this request exceeds the maximum PDU data length (252 bytes).
        // 1 byte for the main Byte Count field + current payload + new request size.
        if 1 + current_payload_size + new_sub_req_size > MAX_PDU_DATA_LEN {
            return Err(MbusError::FileReadPduOverflow);
        }
        self.params
            .push(SubRequestParams {
                file_number,
                record_number,
                record_length,
                record_data: Some(record_data),
            })
            .map_err(|_| MbusError::TooManyFileReadSubRequests)?;

        self.total_read_bytes_length += record_length;
        Ok(())
    }

    /// Calculates the total byte count for the sub-requests payload.
    pub fn byte_count(&self) -> usize {
        self.params
            .iter()
            .map(|p| {
                // 7 bytes for sub-request header + data bytes (if any)
                7 + p.record_data.as_ref().map(|d| d.len() * 2).unwrap_or(0)
            })
            .sum()
    }

    /// Clears all sub-requests.
    pub fn clear_all(&mut self) {
        self.total_read_bytes_length = 0;
        self.params.clear();
    }
}


#[derive(Debug, Clone)]
pub struct FileRecordService;

impl FileRecordService {
    /// Creates a new instance of `FileRecordService`.
    pub fn new() -> Self {
        Self
    }

    /// Sends a Read File Record request.
    pub fn read_file_record(
        &self,
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = FileRecordReqPdu::read_file_record_request(sub_request)?;
        self.build_adu(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write File Record request.
    pub fn write_file_record(
        &self,
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = FileRecordReqPdu::write_file_record_request(sub_request)?;
        self.build_adu(txn_id, unit_id, pdu, transport_type)
    }

    /// Helper to build the ADU from PDU based on transport type.
    fn build_adu(
        &self,
        txn_id: u16,
        unit_id: u8,
        pdu: Pdu,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        match transport_type {
            TransportType::StdTcp | TransportType::CustomTcp => {
                let pdu_bytes_len = pdu.to_bytes()?.len() as u16;
                let mbap_header = MbapHeader::new(txn_id, pdu_bytes_len + 1, unit_id);
                ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu).to_bytes()
            }
            TransportType::StdSerial | TransportType::CustomSerial => {
                todo!("Serial transport ADU construction not implemented")
            }
        }
    }

    /// Handles a Read File Record response.
    pub fn handle_read_file_record_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
        if function_code != FunctionCode::ReadFileRecord {
            return Err(MbusError::InvalidFunctionCode);
        }
        FileRecordReqPdu::parse_read_file_record_response(pdu)
    }

    /// Handles a Write File Record response.
    pub fn handle_write_file_record_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteFileRecord {
            return Err(MbusError::InvalidFunctionCode);
        }
        FileRecordReqPdu::parse_write_file_record_response(pdu)
    }
}

impl PduDataBytes for SubRequest {
    fn to_sub_req_pdu_bytes(&self) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError> {
        let mut bytes = Vec::new();
        // Byte Count: 1 byte (0x07 to 0xF5 bytes)
        let byte_count = self.byte_count();
        bytes
            .push(byte_count as u8)
            .map_err(|_| MbusError::BufferTooSmall)?;

        for param in &self.params {
            // Reference Type: 1 byte (0x06)
            bytes.push(FILE_RECORD_REF_TYPE).map_err(|_| MbusError::BufferTooSmall)?;
            bytes
                .extend_from_slice(&param.file_number.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
            bytes
                .extend_from_slice(&param.record_number.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
            bytes
                .extend_from_slice(&param.record_length.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
            if let Some(ref data) = param.record_data {
                for val in data {
                    bytes
                        .extend_from_slice(&val.to_be_bytes())
                        .map_err(|_| MbusError::BufferLenMissmatch)?;
                }
            }
        }
        Ok(bytes)
    }
}

/// Helper struct for creating and parsing File Record PDUs.
pub struct FileRecordReqPdu {}

impl FileRecordReqPdu {
    /// Creates a Read File Record (FC 0x14) request PDU.
    pub fn read_file_record_request(sub_request: &SubRequest) -> Result<Pdu, MbusError> {
        let data_bytes = sub_request.to_sub_req_pdu_bytes()?;
        let data_bytes_len = data_bytes.len() as u8;
        Ok(Pdu::new(
            FunctionCode::ReadFileRecord,
            data_bytes,
            data_bytes_len,
        ))
    }

    /// Creates a Write File Record (FC 0x15) request PDU.
    pub fn write_file_record_request(sub_request: &SubRequest) -> Result<Pdu, MbusError> {
        let data_bytes = sub_request.to_sub_req_pdu_bytes()?;
        let data_bytes_len = data_bytes.len() as u8;
        Ok(Pdu::new(
            FunctionCode::WriteFileRecord,
            data_bytes,
            data_bytes_len,
        ))
    }

    /// Parses a Read File Record (FC 0x14) response PDU.
    pub fn parse_read_file_record_response(
        pdu: &Pdu,
    ) -> Result<Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadFileRecord {
            return Err(MbusError::ParseError);
        }
        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let byte_count = data[0] as usize;
        // Check if data length matches byte count + 1 (for the byte count field itself)
        if data.len() != byte_count + 1 {
            return Err(MbusError::InvalidPduLength);
        }

        let mut sub_requests = Vec::new();
        let mut i = 1;

        while i < data.len() {
            // Expect at least File Resp Len (1 byte) + Ref Type (1 byte) = 2 bytes
            if i + 2 > data.len() {
                return Err(MbusError::ParseError);
            }

            let file_resp_len = data[i] as usize;
            let ref_type = data[i + 1];

            if ref_type != FILE_RECORD_REF_TYPE {
                return Err(MbusError::ParseError);
            }

            // file_resp_len includes the ref_type byte.
            if file_resp_len < 1 {
                return Err(MbusError::ParseError);
            }
            let data_len = file_resp_len - 1;

            // Check if the sub-response fits in the buffer
            // i + 1 (len byte) + file_resp_len
            if i + 1 + file_resp_len > data.len() {
                return Err(MbusError::ParseError);
            }

            // Extract data bytes: skip len byte (1) + ref type byte (1)
            let raw_data = &data[i + 2..i + 2 + data_len];
            if raw_data.len() % 2 != 0 {
                return Err(MbusError::ParseError);
            }

            let mut values = Vec::new();
            for chunk in raw_data.chunks(2) {
                let val = u16::from_be_bytes([chunk[0], chunk[1]]);
                values.push(val).map_err(|_| MbusError::BufferTooSmall)?;
            }

            let params = SubRequestParams {
                file_number: 0, // Not returned in response
                record_number: 0, // Not returned in response
                record_length: values.len() as u16,
                record_data: Some(values),
            };
            sub_requests.push(params).map_err(|_| MbusError::BufferTooSmall)?;

            // Move to next sub-response: current index + 1 (len byte) + length of sub-response
            i += 1 + file_resp_len;
        }

        Ok(sub_requests)
    }

    /// Parses a Write File Record (FC 0x15) response PDU.
    pub fn parse_write_file_record_response(pdu: &Pdu) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteFileRecord {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        // Byte Count: 1 byte indicating the length of the response data that follows
        let byte_count = data[0] as usize;
        if data.len() != byte_count + 1 {
            return Err(MbusError::InvalidPduLength);
        }

        let mut i = 1;
        while i < data.len() {
            // Fixed header part: Ref(1) + File(2) + RecNum(2) + RecLen(2) = 7 bytes
            if i + SUB_REQ_PARAM_BYTE_LEN > data.len() {
                return Err(MbusError::InvalidPduLength);
            }

            let ref_type = data[i];
            if ref_type != FILE_RECORD_REF_TYPE {
                return Err(MbusError::ParseError);
            }

            // Record length is at offset 5 and 6 relative to the start of the sub-request
            let record_len = u16::from_be_bytes([data[i + 5], data[i + 6]]) as usize;
            let data_byte_len = record_len * 2;

            let sub_req_len = SUB_REQ_PARAM_BYTE_LEN + data_byte_len;
            if i + sub_req_len > data.len() {
                return Err(MbusError::ParseError);
            }

            i += sub_req_len;
        }

        Ok(())
    } 
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_codes::public::FunctionCode;

    // --- Read File Record (FC 20) Tests ---

    #[test]
    fn test_read_file_record_request_valid() {
        let mut sub_req = SubRequest::new();
        // Read 2 registers from File 4, Record 1
        sub_req.add_read_sub_request(4, 1, 2).unwrap();

        let pdu = FileRecordReqPdu::read_file_record_request(&sub_req).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::ReadFileRecord);
        // Expected Data:
        // Byte Count (1 byte) = 7 (0x07)
        // Sub-Req: Ref(0x06), File(0x0004), RecNum(0x0001), RecLen(0x0002)
        let expected_data = [0x07, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02];
        assert_eq!(pdu.data().as_slice(), &expected_data);
    }

    #[test]
    fn test_read_file_record_request_multiple_sub_requests() {
        let mut sub_req = SubRequest::new();
        sub_req.add_read_sub_request(4, 1, 2).unwrap(); // 7 bytes
        sub_req.add_read_sub_request(5, 10, 1).unwrap(); // 7 bytes

        let pdu = FileRecordReqPdu::read_file_record_request(&sub_req).unwrap();

        // Byte Count = 14 (0x0E)
        let expected_data = [
            0x0E, // Byte Count
            0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, // Sub-Req 1
            0x06, 0x00, 0x05, 0x00, 0x0A, 0x00, 0x01, // Sub-Req 2
        ];
        assert_eq!(pdu.data().as_slice(), &expected_data);
    }

    #[test]
    fn test_read_file_record_overflow_protection() {
        let mut sub_req = SubRequest::new();
        // Max registers allowed in response is ~125 (depending on N sub-requests).
        // Formula: N + TotalRegs <= 125.
        // Let's add 1 sub-request asking for 124 registers. 1 + 124 = 125. OK.
        sub_req.add_read_sub_request(1, 0, 124).unwrap();

        // Try to add another sub-request for 1 register.
        // New N=2. New TotalRegs=125. 2 + 125 = 127 > 125. Should fail.
        let err = sub_req.add_read_sub_request(1, 124, 1).unwrap_err();
        assert_eq!(err, MbusError::FileReadPduOverflow);
    }

    #[test]
    fn test_parse_read_file_record_response_valid() {
        // Response for reading 2 registers (0x1234, 0x5678)
        // Byte Count: 1 (Len) + 1 (Ref) + 4 (Data) = 6
        // PDU Data: [0x06, 0x05 (Len), 0x06 (Ref), 0x12, 0x34, 0x56, 0x78]
        let data = [0x06, 0x05, 0x06, 0x12, 0x34, 0x56, 0x78];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::ReadFileRecord, pdu_data, data.len() as u8);

        let sub_reqs = FileRecordReqPdu::parse_read_file_record_response(&pdu).unwrap();
        assert_eq!(sub_reqs.len(), 1);
        let data = sub_reqs[0].record_data.as_ref().unwrap();
        assert_eq!(data.as_slice(), &[0x1234, 0x5678]);
        assert_eq!(sub_reqs[0].record_length, 2);
    }

    #[test]
    fn test_parse_read_file_record_response_malformed() {
        // Invalid Ref Type (0x07 instead of 0x06)
        let data = [0x06, 0x05, 0x07, 0x12, 0x34, 0x56, 0x78];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::ReadFileRecord, pdu_data, data.len() as u8);

        let err = FileRecordReqPdu::parse_read_file_record_response(&pdu).unwrap_err();
        assert_eq!(err, MbusError::ParseError);
    }

    // --- Write File Record (FC 21) Tests ---

    #[test]
    fn test_write_file_record_request_valid() {
        let mut sub_req = SubRequest::new();
        let mut data = Vec::new();
        data.push(0x1234).unwrap();
        data.push(0x5678).unwrap();

        // Write 2 registers to File 4, Record 1
        sub_req
            .add_write_sub_request(4, 1, 2, data.clone())
            .unwrap();

        let pdu = FileRecordReqPdu::write_file_record_request(&sub_req).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::WriteFileRecord);
        // Expected Data:
        // Byte Count = 7 (Header) + 4 (Data) = 11 (0x0B)
        // Sub-Req: Ref(06), File(00 04), Rec(00 01), Len(00 02), Data(12 34 56 78)
        let expected_data = [
            0x0B, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78,
        ];
        assert_eq!(pdu.data().as_slice(), &expected_data);
    }

    #[test]
    fn test_write_file_record_request_data_len_mismatch() {
        let mut sub_req = SubRequest::new();
        let mut data = Vec::new();
        data.push(0x1234).unwrap();

        // Record length says 2, but data has 1
        let err = sub_req
            .add_write_sub_request(4, 1, 2, data)
            .unwrap_err();
        assert_eq!(err, MbusError::BufferLenMissmatch);
    }

    #[test]
    fn test_write_file_record_overflow_protection() {
        let mut sub_req = SubRequest::new();
        // Max PDU data is 252.
        // Overhead: 1 (Byte Count).
        // Sub-Req Overhead: 7 bytes.
        // Available for data: 252 - 1 - 7 = 244 bytes = 122 registers.

        let mut data = Vec::new();
        for _ in 0..122 {
            data.push(0xFFFF).unwrap();
        }

        // This should fit exactly (1 + 7 + 244 = 252)
        sub_req
            .add_write_sub_request(1, 1, 122, data.clone())
            .unwrap();

        // Adding another small request should fail
        let mut small_data = Vec::new();
        small_data.push(0x0000).unwrap();
        let err = sub_req
            .add_write_sub_request(2, 2, 1, small_data)
            .unwrap_err();

        assert_eq!(err, MbusError::FileReadPduOverflow);
    }

    #[test]
    fn test_parse_write_file_record_response_valid() {
        // Response is an echo of the request.
        // Byte Count = 11 (0x0B)
        // Sub-Req: Ref(06), File(00 04), Rec(00 01), Len(00 02), Data(12 34 56 78)
        let data = [
            0x0B, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78,
        ];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::WriteFileRecord, pdu_data, data.len() as u8);

        let result = FileRecordReqPdu::parse_write_file_record_response(&pdu);
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_write_file_record_response_invalid_structure() {
        // Response claims length 2, but data is missing
        // Byte Count = 11 (0x0B)
        // Sub-Req: Ref(06), File(00 04), Rec(00 01), Len(00 02), Data(MISSING)
        let data = [0x0B, 0x06, 0x00, 0x04, 0x00, 0x01, 0x00, 0x02];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::WriteFileRecord, pdu_data, data.len() as u8);

        let err = FileRecordReqPdu::parse_write_file_record_response(&pdu).unwrap_err();
        // The parser loop checks `i + sub_req_len > data.len()`.
        // i=1. sub_req_len = 7 + 4 = 11.
        // 1 + 11 = 12. data.len() = 8.
        // 12 > 8 -> Error.
        assert_eq!(err, MbusError::InvalidPduLength);
    }

    // --- FileRecordService Tests ---

    /// Test case: `read_file_record` correctly constructs a Modbus TCP ADU.
    #[test]
    fn test_file_record_service_read_request_tcp() {
        let service = FileRecordService::new();
        let mut sub_req = SubRequest::new();
        // Read 2 registers from File 4, Record 1
        sub_req.add_read_sub_request(4, 1, 2).unwrap();

        let txn_id = 0x1234;
        let unit_id = 0x01;
        let adu = service
            .read_file_record(txn_id, unit_id, &sub_req, TransportType::StdTcp)
            .unwrap();

        // Expected ADU:
        // MBAP: TID(12 34) PID(00 00) Len(00 0A) Unit(01)
        // PDU: FC(14) ByteCount(07) Ref(06) File(00 04) Rec(00 01) Len(00 02)
        // MBAP Len = 1 (Unit) + 1 (FC) + 1 (ByteCount) + 7 (SubReq) = 10 (0x0A)
        let expected = [
            0x12, 0x34, 0x00, 0x00, 0x00, 0x0A, 0x01, 0x14, 0x07, 0x06, 0x00, 0x04, 0x00, 0x01,
            0x00, 0x02,
        ];
        assert_eq!(adu.as_slice(), &expected);
    }

    /// Test case: `write_file_record` correctly constructs a Modbus TCP ADU.
    #[test]
    fn test_file_record_service_write_request_tcp() {
        let service = FileRecordService::new();
        let mut sub_req = SubRequest::new();
        let mut data = Vec::new();
        data.push(0x1122).unwrap();
        // Write 1 register (0x1122) to File 4, Record 1
        sub_req
            .add_write_sub_request(4, 1, 1, data)
            .unwrap();

        let txn_id = 0x5678;
        let unit_id = 0x02;
        let adu = service
            .write_file_record(txn_id, unit_id, &sub_req, TransportType::StdTcp)
            .unwrap();

        // Expected ADU:
        // MBAP: TID(56 78) PID(00 00) Len(00 0C) Unit(02)
        // PDU: FC(15) ByteCount(09) Ref(06) File(00 04) Rec(00 01) Len(00 01) Data(11 22)
        // SubReq Len = 7 (Header) + 2 (Data) = 9.
        // MBAP Len = 1 (Unit) + 1 (FC) + 1 (ByteCount) + 9 (SubReq) = 12 (0x0C)
        let expected = [
            0x56, 0x78, 0x00, 0x00, 0x00, 0x0C, 0x02, 0x15, 0x09, 0x06, 0x00, 0x04, 0x00, 0x01,
            0x00, 0x01, 0x11, 0x22,
        ];
        assert_eq!(adu.as_slice(), &expected);
    }

    /// Test case: `handle_read_file_record_rsp` correctly parses a valid response PDU.
    #[test]
    fn test_file_record_service_handle_read_response() {
        let service = FileRecordService::new();
        // Response PDU for Read File Record:
        // FC(14) + ByteCount(04) + SubRespLen(03) + Ref(06) + Data(AA BB)
        let data = [0x04, 0x03, 0x06, 0xAA, 0xBB];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::ReadFileRecord, pdu_data, 5);

        let result = service.handle_read_file_record_rsp(FunctionCode::ReadFileRecord, &pdu);
        assert!(result.is_ok());
        let sub_reqs = result.unwrap();
        assert_eq!(sub_reqs.len(), 1);
        assert_eq!(sub_reqs[0].record_data.as_ref().unwrap().as_slice(), &[0xAABB]);
    }

    /// Test case: `handle_read_file_record_rsp` returns error for incorrect function code.
    #[test]
    fn test_file_record_service_handle_read_response_wrong_fc() {
        let service = FileRecordService::new();
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = service.handle_read_file_record_rsp(FunctionCode::ReadCoils, &pdu);
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }
    
}
