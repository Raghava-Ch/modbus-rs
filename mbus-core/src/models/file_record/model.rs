//! # Modbus File Record Models
//!
//! This module provides the data structures for handling **Read File Record** (Function Code 0x14)
//! and **Write File Record** (Function Code 0x15).
//!
//! File records allow access to a large, structured memory area organized into files and records.
//! Unlike coils or registers, file record operations are composed of one or more "sub-requests"
//! within a single Modbus PDU.
//!
//! ## Key Components
//! - [`SubRequest`]: A container that aggregates multiple read or write operations into a single PDU.
//! - [`SubRequestParams`]: The specific parameters (File No, Record No, Length, Data) for an individual operation.
//! - [`MAX_SUB_REQUESTS_PER_PDU`]: The protocol limit of 35 sub-requests per frame.
//!
//! ## Constraints
//! - The total size of all sub-requests (including headers and data) must not exceed the
//!   maximum Modbus PDU data length of 252 bytes.
//! - For Read requests, the response size is also validated during sub-request addition to
//!   ensure the server can fit the requested data into a single response PDU.

use crate::{data_unit::common::MAX_PDU_DATA_LEN, errors::MbusError};
use heapless::Vec;

/// Maximum number of sub-requests allowed in a single PDU (35).
pub const MAX_SUB_REQUESTS_PER_PDU: usize = 35;
/// Byte count is 1 byte for each sub-request
///(reference type + file number + record number + record length) + 1 byte for the byte count itself
pub const SUB_REQ_PARAM_BYTE_LEN: usize = 6 + 1;
/// The reference type for file record requests (0x06).
pub const FILE_RECORD_REF_TYPE: u8 = 0x06;

/// A trait for converting Modbus PDU data structures into a byte vector.
///
/// This is specifically used for file record sub-requests to serialize their parameters
/// into the format expected within a Modbus PDU.
pub trait PduDataBytes {
    /// Converts the sub-request parameters into a byte vector for the PDU.
    fn to_sub_req_pdu_bytes(&self) -> Result<Vec<u8, MAX_PDU_DATA_LEN>, MbusError>;
}

/// Parameters for a single file record sub-request.
#[derive(Debug, Clone, PartialEq)]
pub struct SubRequestParams {
    /// The file number to be read/written (0x0001 to 0xFFFF).
    /// In Modbus, files are logical groupings of records.
    pub file_number: u16,
    /// The starting record number within the file (0x0000 to 0x270F).
    /// Each record is typically 2 bytes (one 16-bit register).
    pub record_number: u16,
    /// The length of the record in number of 16-bit registers.
    /// For Read (FC 0x14), this is the amount to retrieve.
    /// For Write (FC 0x15), this must match the length of `record_data`.
    pub record_length: u16,
    /// The actual register values to be written to the file.
    /// This field is `Some` for Write File Record (FC 0x15) and `None` for Read File Record (FC 0x14).
    /// The data is stored in a `heapless::Vec` to ensure `no_std` compatibility.
    pub record_data: Option<Vec<u16, MAX_PDU_DATA_LEN>>,
}

/// Represents a collection of sub-requests for Modbus File Record operations.
///
/// A single Modbus PDU for FC 0x14 or 0x15 can contain multiple sub-requests,
/// allowing the client to read from or write to different files and records in one transaction.
///
/// This struct manages the aggregation of these requests and performs validation to ensure
/// the resulting PDU does not exceed the Modbus protocol limit of 253 bytes.
#[derive(Debug, Clone, Default)]
pub struct SubRequest {
    /// A fixed-capacity vector of individual sub-request parameters.
    /// The capacity is limited to 35 as per the Modbus specification.
    params: Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>,
    /// The cumulative count of registers (16-bit words) requested for reading.
    /// Used to calculate and validate the expected response size.
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
            bytes
                .push(FILE_RECORD_REF_TYPE)
                .map_err(|_| MbusError::BufferTooSmall)?;
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
