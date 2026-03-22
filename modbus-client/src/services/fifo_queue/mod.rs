//! # Modbus FIFO Queue Service
//!
//! This module implements the client-side logic for **Function Code 24 (0x18): Read FIFO Queue**.
//!
//! The Read FIFO Queue function allows the client to read the contents of a First-In-First-Out (FIFO)
//! queue of 16-bit registers in a remote device. The function returns a count of the registers
//! in the queue, followed by the queue data.
//!
//! ## Module Structure
//! - `apis`: High-level public API for triggering FIFO queue reads via `ClientServices`.
//! - `request`: Handles the construction and serialization of the Read FIFO Queue request PDU.
//! - `response`: Handles parsing, validation of byte counts, and register extraction from response PDUs.
//! - `service`: Internal orchestration logic for building ADUs and handling de-encapsulation.
//!
//! ## Constraints
//! - **Max Capacity**: The Modbus specification limits the FIFO queue to a maximum of 31 registers.
//! - **Read-Only**: This function code is specifically for reading; writing to FIFOs is typically
//!   handled via standard register write functions (FC 06 or 16) depending on the server implementation.
//! - **no_std**: Fully compatible with embedded environments using fixed-size buffers.

pub mod request;
pub mod response;

pub use mbus_core::models::fifo_queue::*;

mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::services::fifo_queue::{request::ReqPduCompiler, response::ResponseParser};
    use mbus_core::{
        data_unit::common::Pdu, errors::MbusError, function_codes::public::FunctionCode,
    };

    // --- Read FIFO Queue (FC 0x18) ---

    /// Test case: `read_fifo_queue_request` with valid data.
    #[test]
    fn test_read_fifo_queue_request_valid() {
        let pdu = ReqPduCompiler::read_fifo_queue_request(0x0001).unwrap();
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
        let (values, count) = ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap();
        assert_eq!(&values[..count], &[0x1234]);
    }

    /// Test case: `parse_read_fifo_queue_response` successfully parses a valid response with multiple registers.
    #[test]
    fn test_parse_read_fifo_queue_response_multiple_registers() {
        // Response: FC(0x18), FIFO Byte Count(0x0006), FIFO Count(0x0002), FIFO Value(0x1234, 0x5678)
        // PDU data: [0x00, 0x06, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78]
        let response_bytes = [0x18, 0x00, 0x06, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let (values, count) = ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap();
        assert_eq!(&values[..count], &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_fifo_queue_response_wrong_fc() {
        let response_bytes = [0x03, 0x00, 0x04, 0x00, 0x01, 0x12, 0x34]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for PDU data too short.
    #[test]
    fn test_parse_read_fifo_queue_response_data_too_short() {
        let response_bytes = [0x18, 0x00, 0x04, 0x00]; // Missing FIFO Count and values
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap_err(),
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
            ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_read_fifo_queue_response` returns an error for FIFO count mismatch.
    #[test]
    fn test_parse_read_fifo_queue_response_fifo_count_mismatch() {
        let response_bytes = [0x18, 0x00, 0x04, 0x00, 0x02, 0x12, 0x34]; // FIFO Count 2, but only 1 register value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            ResponseParser::parse_read_fifo_queue_response(&pdu).unwrap_err(),
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
