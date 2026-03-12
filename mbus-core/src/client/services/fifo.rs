//! Modbus FIFO Queue Service Module
//!
//! This module provides the necessary structures and logic to handle Modbus operations
//! related to FIFO Queues (Function Code 0x18).
//!
//! It includes functionality for:
//! - Reading the contents of a remote FIFO queue of registers.
//! - Parsing response PDUs containing the FIFO count and register values.
//! - Validating data integrity (byte counts vs register counts).
//!
//! This module is designed for `no_std` environments using `heapless` collections.
//! The maximum number of registers in a single FIFO response is limited to 31 by the protocol.
 
use heapless::Vec;

use crate::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu},
    errors::MbusError,
    function_codes::public::{FunctionCode, MAX_PDU_DATA_LEN},
    transport::TransportType,
};

/// The maximum number of bytes that can be returned in a FIFO Queue response PDU's data section.
pub const MAX_FIFO_QUEUE_COUNT_PER_PDU: usize = 31;

/// Represents a Modbus FIFO Queue response.
#[derive(Debug, Clone)]
pub struct FifoQueue {
    /// The FIFO pointer address.
    pub ptr_address: u16,
    /// The values read from the FIFO queue.
    pub values: Vec<u16, MAX_FIFO_QUEUE_COUNT_PER_PDU>,
}

impl FifoQueue {
    /// Creates a new `FifoQueue` instance with the given pointer address and an empty values vector.
    pub fn new(ptr_address: u16) -> Self {
        Self {
            ptr_address,
            values: Vec::new(),
        }
    }

    /// Sets the values of the FIFO queue.
    pub fn with_values(mut self, values: Vec<u16, MAX_FIFO_QUEUE_COUNT_PER_PDU>) -> Self {
        self.values = values;
        self
    }
}

/// Provides service operations for reading Modbus FIFO Queue.
#[derive(Debug, Clone)]
pub struct FifoQueueService;

impl FifoQueueService {
    /// Creates a new `FifoQueueService`.
    pub fn new() -> Self {
        Self
    }

    /// Sends a Read FIFO Queue request.
    pub fn read_fifo_queue(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = FifoQueueReqPdu::read_fifo_queue_request(address)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Handles a Read FIFO Queue response.
    pub fn handle_read_fifo_queue_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<FifoQueue, MbusError> {
        if function_code != FunctionCode::ReadFifoQueue {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = FifoQueueReqPdu::parse_read_fifo_queue_response(pdu)?;
        Ok(FifoQueue::new(0).with_values(values))
    }
}

/// Provides operations for creating and parsing Modbus FIFO Queue request/response PDUs.
pub struct FifoQueueReqPdu {}

impl FifoQueueReqPdu {
    /// Creates a Modbus Read FIFO Queue request PDU.
    pub fn read_fifo_queue_request(address: u16) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadFifoQueue, data_vec, 2)) // Corrected: 2 addr
    }

    /// Parses the received response for a Modbus Read FIFO Queue request.
    pub fn parse_read_fifo_queue_response(
        pdu: &Pdu,
    ) -> Result<Vec<u16, MAX_FIFO_QUEUE_COUNT_PER_PDU>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadFifoQueue {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        // PDU Data: FIFO Byte Count (2 bytes), FIFO Count (2 bytes), N * Register Value (2 bytes each)
        if data.len() < 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let fifo_byte_count = u16::from_be_bytes([data[0], data[1]]) as usize;
        // The total data length should be 2 (for the byte count field itself) + fifo_byte_count.
        if data.len() != 2 + fifo_byte_count {
            return Err(MbusError::InvalidPduLength);
        }

        let fifo_count = u16::from_be_bytes([data[2], data[3]]) as usize;

        // The byte count should be 2 bytes for the FIFO count field, plus 2 bytes for each register.
        if fifo_byte_count != 2 + fifo_count * 2 {
            return Err(MbusError::ParseError);
        }

        // FIFO Count is at data[2..4]
        // Values start at data[4..]
        let mut values = Vec::new();
        for chunk in data[4..].chunks_exact(2) {
            let val = u16::from_be_bytes([chunk[0], chunk[1]]);
            values
                .push(val)
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        if values.len() != fifo_count {
            return Err(MbusError::ParseError);
        }

        Ok(values)
    }
}

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::{data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode};

    use super::*;
    // --- Read FIFO Queue (FC 0x18) ---

    /// Test case: `read_fifo_queue_request` with valid data.
    #[test]
    fn test_read_fifo_queue_request_valid() {
        let pdu = FifoQueueReqPdu::read_fifo_queue_request(0x0001).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadFifoQueue);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x01]);
        assert_eq!(pdu.data_len(), 2);
    }

    // --- Parse Read FIFO Queue Response Tests ---

    /// Test case: `parse_read_fifo_queue_response` successfully parses a valid response with data.
    #[test]
    fn test_parse_read_fifo_queue_response_valid() {
        // Response: FC(0x18), FIFO Byte Count(0x0004), FIFO Count(0x0001), FIFO Value(0x1234)
        // PDU data: [0x00, 0x04, 0x00, 0x01, 0x12, 0x34]
        let response_bytes = [0x18, 0x00, 0x04, 0x00, 0x01, 0x12, 0x34];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers = FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234]);
    }

    /// Test case: `parse_read_fifo_queue_response` successfully parses a valid response with multiple registers.
    #[test]
    fn test_parse_read_fifo_queue_response_multiple_registers() {
        // Response: FC(0x18), FIFO Byte Count(0x0006), FIFO Count(0x0002), FIFO Value(0x1234, 0x5678)
        // PDU data: [0x00, 0x06, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78]
        let response_bytes = [0x18, 0x00, 0x06, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers = FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_fifo_queue_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x04, 0x00, 0x01, 0x12, 0x34]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for PDU data too short.
    #[test]
    fn test_parse_read_fifo_queue_response_data_too_short() {
        let response_bytes = [0x18, 0x00, 0x04, 0x00]; // Missing FIFO Count and values
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for FIFO byte count mismatch.
    #[test]
    fn test_parse_read_fifo_queue_response_fifo_byte_count_mismatch() {
        // Total PDU data length is 7. Byte count is 5. 7 = 2 + 5. Length is correct.
        // FIFO count is 1. Byte count should be 2 + 1*2 = 4. But it is 5. This is a ParseError.
        let response_bytes = [0x18, 0x00, 0x05, 0x00, 0x01, 0x12, 0x34, 0x00];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for FIFO count mismatch.
    #[test]
    fn test_parse_read_fifo_queue_response_fifo_count_mismatch() {
        let response_bytes = [0x18, 0x00, 0x04, 0x00, 0x02, 0x12, 0x34]; // FIFO Count 2, but only 1 register value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            FifoQueueReqPdu::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error if the internal `Vec` capacity is exceeded.
    #[test]
    fn test_parse_read_fifo_queue_response_buffer_too_small_for_data() {
        // This test checks that creating a PDU with more data than the spec allows will fail.
        // A Read FIFO Queue response PDU's data section is:
        // [FIFO Byte Count (2 bytes)] [FIFO Count (2 bytes)] [Values (N*2 bytes)]
        // The total data length is `4 + N*2`. This must be <= MAX_PDU_DATA_LEN (252).
        // This implies max N is 124.
        //
        // This test attempts to create a PDU with N=126, which would mean a data length of
        // 4 + 126*2 = 256 bytes, which is > 252. `Pdu::from_bytes` should reject this.
        let fifo_count = 126;
        let fifo_byte_count = 2 + (fifo_count * 2); // 2 + 252 = 254

        // Use a Vec for test data setup.
        let mut response_pdu_bytes: Vec<u8, 512> = Vec::new();
        for _ in 0..(1 + 2 + 2 + fifo_count * 2) {
            response_pdu_bytes.push(0u8).unwrap();
        }
        response_pdu_bytes[0] = 0x18; // FC
        response_pdu_bytes[1..3].copy_from_slice(&(fifo_byte_count as u16).to_be_bytes());
        response_pdu_bytes[3..5].copy_from_slice(&(fifo_count as u16).to_be_bytes());

        // The data part of the PDU (len 256) is > MAX_PDU_DATA_LEN (252).
        let result = Pdu::from_bytes(&response_pdu_bytes);
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }
}
