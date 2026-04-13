//! # Modbus File Record Response Handling
//!
//! This module provides the logic for parsing and dispatching responses related to
//! Modbus File Records (Function Codes 0x14 and 0x15).
//!
//! ## Responsibilities
//! - **Parsing**: Validates the complex PDU structure of file record responses, which consist of
//!   multiple sub-requests within a single frame.
//! - **De-encapsulation**: Extracts register data from Read File Record responses and validates
//!   echoed metadata for Write File Record responses.
//! - **Dispatching**: Routes the parsed sub-request data to the application layer via the
//!   `FileRecordResponse` trait.
//!
//! ## Architecture
//! - `ResponseParser`: Contains low-level logic to iterate through the byte stream and reconstruct
//!   `SubRequestParams` from the bitstream.
//! - `ClientServices` implementation: Orchestrates the high-level handling, converting the PDU
//!   into a collection of sub-requests and triggering the appropriate application callback.

use heapless::Vec;

use crate::{
    app::FileRecordResponse,
    services::file_record::{
        FILE_RECORD_REF_TYPE, MAX_SUB_REQUESTS_PER_PDU, SUB_REQ_PARAM_BYTE_LEN,
    },
    services::{
        ClientCommon, ClientServices, ExpectedResponse,
        file_record::{self, SubRequestParams},
    },
};
use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::Transport,
};

/// # ResponseParser
///
/// A low-level utility for decoding Modbus Protocol Data Units (PDUs) specific to File Record operations.
///
/// This struct provides stateless methods to validate function codes (0x14, 0x15), verify byte counts,
/// and extract sub-request data from raw byte buffers. It handles the complex, variable-length
/// structure of file record responses which can contain multiple data blocks in a single frame.
pub(super) struct ResponseParser;

impl ResponseParser {
    /// Parses a Read File Record (FC 0x14) response PDU.
    pub(super) fn parse_read_file_record_response(
        pdu: &Pdu,
    ) -> Result<Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadFileRecord {
            return Err(MbusError::ParseError);
        }
        let bcp = pdu.byte_count_payload()?;
        let mut sub_requests = Vec::new();
        let mut i = 0;

        while i < bcp.payload.len() {
            // Expect at least File Resp Len (1 byte) + Ref Type (1 byte) = 2 bytes
            if i + 2 > bcp.payload.len() {
                return Err(MbusError::ParseError);
            }

            let file_resp_len = bcp.payload[i] as usize;
            let ref_type = bcp.payload[i + 1];

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
            if i + 1 + file_resp_len > bcp.payload.len() {
                return Err(MbusError::ParseError);
            }

            // Extract data bytes: skip len byte (1) + ref type byte (1)
            let raw_data = &bcp.payload[i + 2..i + 2 + data_len]; // raw_data is the actual register data
            if !raw_data.len().is_multiple_of(2) {
                // The length of register data must be a multiple of 2
                return Err(MbusError::ParseError);
            }

            let mut values = Vec::new();
            for chunk in raw_data.chunks(2) {
                let val = u16::from_be_bytes([chunk[0], chunk[1]]);
                values.push(val).map_err(|_| MbusError::BufferTooSmall)?;
            }

            let params = SubRequestParams {
                file_number: 0,   // Not returned in response
                record_number: 0, // Not returned in response
                record_length: values.len() as u16,
                record_data: Some(values),
            };
            sub_requests
                .push(params)
                .map_err(|_| MbusError::BufferTooSmall)?;

            // Move to next sub-response: current index + 1 (len byte) + length of sub-response
            i += 1 + file_resp_len;
        }

        Ok(sub_requests)
    }

    /// Parses a Write File Record (FC 0x15) response PDU.
    pub(super) fn parse_write_file_record_response(pdu: &Pdu) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteFileRecord {
            return Err(MbusError::ParseError);
        }

        let bcp = pdu.byte_count_payload()?;
        let mut i = 0;
        while i < bcp.payload.len() {
            // Fixed header part: Ref(1) + File(2) + RecNum(2) + RecLen(2) = 7 bytes
            if i + SUB_REQ_PARAM_BYTE_LEN > bcp.payload.len() {
                return Err(MbusError::InvalidPduLength);
            }

            let ref_type = bcp.payload[i];
            if ref_type != FILE_RECORD_REF_TYPE {
                return Err(MbusError::ParseError);
            }

            // Record length is at offset 5 and 6 relative to the start of the sub-request
            let record_len = u16::from_be_bytes([bcp.payload[i + 5], bcp.payload[i + 6]]) as usize;
            let data_byte_len = record_len * 2;

            let sub_req_len = SUB_REQ_PARAM_BYTE_LEN + data_byte_len;
            if i + sub_req_len > bcp.payload.len() {
                return Err(MbusError::ParseError);
            }

            i += sub_req_len;
        }

        Ok(())
    }
}

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + FileRecordResponse,
{
    pub(super) fn handle_read_file_record_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let function_code = message.function_code();
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let data = match file_record::service::ServiceDecompiler::handle_read_file_record_rsp(
            function_code,
            pdu,
        ) {
            Ok(d) => d,
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
                return;
            }
        };

        self.app
            .read_file_record_response(transaction_id, unit_id_or_slave_addr, &data);
    }

    pub(super) fn handle_write_file_record_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let function_code = message.function_code();
        let pdu = message.pdu();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        if file_record::service::ServiceDecompiler::handle_write_file_record_rsp(function_code, pdu)
            .is_ok()
        {
            self.app
                .write_file_record_response(transaction_id, unit_id_or_slave_addr);
        } else {
            self.app
                .request_failed(transaction_id, unit_id_or_slave_addr, MbusError::ParseError);
        }
    }
}
