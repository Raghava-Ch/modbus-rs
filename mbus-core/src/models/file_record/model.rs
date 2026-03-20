use crate::{data_unit::common::MAX_PDU_DATA_LEN, errors::MbusError};
use heapless::Vec;

/// Maximum number of sub-requests allowed in a single PDU (35).
pub const MAX_SUB_REQUESTS_PER_PDU: usize = 35;
/// Byte count is 1 byte for each sub-request
///(reference type + file number + record number + record length) + 1 byte for the byte count itself
pub const SUB_REQ_PARAM_BYTE_LEN: usize = 6 + 1;
/// The reference type for file record requests (0x06).
pub const FILE_RECORD_REF_TYPE: u8 = 0x06;

pub trait PduDataBytes {
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
    /// A vector of individual sub-request parameters.
    params: Vec<SubRequestParams, MAX_SUB_REQUESTS_PER_PDU>, // maximum of 35 sub-requests per PDU
    /// The total length of data in bytes that will be read across all sub-requests.
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
