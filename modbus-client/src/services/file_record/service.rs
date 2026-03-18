use heapless::Vec;

use crate::{
    services::file_record::MAX_SUB_REQUESTS_PER_PDU,
    services::file_record::{
        request::ReqPduCompiler,
        response::ResponseParser,
        {SubRequest, SubRequestParams},
    },
};
use mbus_core::{data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu},
errors::MbusError,
function_codes::public::FunctionCode,
transport::TransportType,};

/// Provides operations for creating and parsing Modbus File Record request/response PDUs.
#[derive(Debug, Clone)]
pub struct ServiceBuilder;

impl ServiceBuilder {
    /// Sends a Read File Record request.
    pub fn read_file_record(
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_file_record_request(sub_request)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write File Record request.
    pub fn write_file_record(
        txn_id: u16,
        unit_id: u8,
        sub_request: &SubRequest,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::write_file_record_request(sub_request)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }
}

/// Provides operations for creating and parsing Modbus File Record request/response PDUs.
#[derive(Debug, Clone)]
pub struct ServiceDecompiler;

impl ServiceDecompiler {
    /// Handles a Read File Record response.
    pub fn handle_read_file_record_rsp(
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
        if function_code != FunctionCode::ReadFileRecord {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_read_file_record_response(pdu)
    }

    /// Handles a Write File Record response.
    pub fn handle_write_file_record_rsp(
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteFileRecord {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_write_file_record_response(pdu)
    }
}
