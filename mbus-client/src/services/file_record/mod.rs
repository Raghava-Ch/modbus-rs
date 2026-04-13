//! # Modbus File Record Services
//!
//! This module provides the implementation for handling Modbus operations related
//! to generalized File Records.
//!
//! ## Supported Function Codes
//! - **Read File Record (FC 0x14)**: Reads structured data from specific file and record references.
//! - **Write File Record (FC 0x15)**: Writes structured data to specific file and record references.

pub mod request;
pub mod response;

pub use mbus_core::models::file_record::*;

mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::services::file_record::{
        SubRequest, request::ReqPduCompiler, response::ResponseParser, service::ServiceBuilder,
        service::ServiceDecompiler,
    };
    use mbus_core::{
        data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode,
        transport::TransportType,
    };

    // --- Read File Record (FC 20) Tests ---

    #[test]
    fn test_read_file_record_request_valid() {
        let mut sub_req = SubRequest::new();
        // Read 2 registers from File 4, Record 1
        sub_req.add_read_sub_request(4, 1, 2).unwrap();

        let pdu = ReqPduCompiler::read_file_record_request(&sub_req).unwrap();

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

        let pdu = ReqPduCompiler::read_file_record_request(&sub_req).unwrap();

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

        let sub_reqs = ResponseParser::parse_read_file_record_response(&pdu).unwrap();
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

        let err = ResponseParser::parse_read_file_record_response(&pdu).unwrap_err();
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

        let pdu = ReqPduCompiler::write_file_record_request(&sub_req).unwrap();

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
        let err = sub_req.add_write_sub_request(4, 1, 2, data).unwrap_err();
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

        let result = ResponseParser::parse_write_file_record_response(&pdu);
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

        let err = ResponseParser::parse_write_file_record_response(&pdu).unwrap_err();
        // byte_count_payload() returns InvalidByteCount when data.len() != 1 + byte_count.
        // i=1. sub_req_len = 7 + 4 = 11.
        // 1 + 11 = 12. data.len() = 8.
        // 12 > 8 -> Error.
        assert_eq!(err, MbusError::InvalidByteCount);
    }

    // --- FileRecordService Tests ---

    /// Test case: `read_file_record` correctly constructs a Modbus TCP ADU.
    #[test]
    fn test_file_record_service_read_request_tcp() {
        let mut sub_req = SubRequest::new();
        // Read 2 registers from File 4, Record 1
        sub_req.add_read_sub_request(4, 1, 2).unwrap();

        let txn_id = 0x1234;
        let unit_id = 0x01;
        let adu =
            ServiceBuilder::read_file_record(txn_id, unit_id, &sub_req, TransportType::StdTcp)
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
        let mut sub_req = SubRequest::new();
        let mut data = Vec::new();
        data.push(0x1122).unwrap();
        // Write 1 register (0x1122) to File 4, Record 1
        sub_req.add_write_sub_request(4, 1, 1, data).unwrap();

        let txn_id = 0x5678;
        let unit_id = 0x02;
        let adu =
            ServiceBuilder::write_file_record(txn_id, unit_id, &sub_req, TransportType::StdTcp)
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
        // Response PDU for Read File Record:
        // FC(14) + ByteCount(04) + SubRespLen(03) + Ref(06) + Data(AA BB)
        let data = [0x04, 0x03, 0x06, 0xAA, 0xBB];
        let mut pdu_data = Vec::new();
        pdu_data.extend_from_slice(&data).unwrap();
        let pdu = Pdu::new(FunctionCode::ReadFileRecord, pdu_data, 5);

        let result =
            ServiceDecompiler::handle_read_file_record_rsp(FunctionCode::ReadFileRecord, &pdu);
        assert!(result.is_ok());
        let sub_reqs = result.unwrap();
        assert_eq!(sub_reqs.len(), 1);
        assert_eq!(
            sub_reqs[0].record_data.as_ref().unwrap().as_slice(),
            &[0xAABB]
        );
    }

    /// Test case: `handle_read_file_record_rsp` returns error for incorrect function code.
    #[test]
    fn test_file_record_service_handle_read_response_wrong_fc() {
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = ServiceDecompiler::handle_read_file_record_rsp(FunctionCode::ReadCoils, &pdu);
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }
}
