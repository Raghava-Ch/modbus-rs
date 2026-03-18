//! Modbus File Record Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to File Records (Function Codes 0x14 and 0x15).
//!
//! It includes functionality for:
//! - Reading multiple file records (FC 0x14) using sub-requests.
//! - Writing multiple file records (FC 0x15) using sub-requests.
//! - Managing sub-request parameters and validating PDU size constraints.
//! - Parsing response PDUs for both read and write operations.
//!
//! This module is designed for `no_std` environments using `heapless` collections.
//! The maximum number of sub-requests per PDU is limited to 35 by the protocol.

use crate::services::file_record::{PduDataBytes, SubRequest};
use mbus_core::{
    data_unit::common::Pdu,
    errors::MbusError,
    function_codes::public::FunctionCode,
};

/// Helper struct for creating and parsing File Record PDUs.
pub(super) struct ReqPduCompiler {}

impl ReqPduCompiler {
    /// Creates a Read File Record (FC 0x14) request PDU.
    pub(super) fn read_file_record_request(sub_request: &SubRequest) -> Result<Pdu, MbusError> {
        let data_bytes = sub_request.to_sub_req_pdu_bytes()?;
        let data_bytes_len = data_bytes.len() as u8;
        Ok(Pdu::new(
            FunctionCode::ReadFileRecord,
            data_bytes,
            data_bytes_len,
        ))
    }

    /// Creates a Write File Record (FC 0x15) request PDU.
    pub(super) fn write_file_record_request(sub_request: &SubRequest) -> Result<Pdu, MbusError> {
        let data_bytes = sub_request.to_sub_req_pdu_bytes()?;
        let data_bytes_len = data_bytes.len() as u8;
        Ok(Pdu::new(
            FunctionCode::WriteFileRecord,
            data_bytes,
            data_bytes_len,
        ))
    }
}
