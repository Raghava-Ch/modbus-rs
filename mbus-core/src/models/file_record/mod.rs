//! # Modbus File Record Models
//!
//! This module provides the data structures and logic for handling **Read File Record**
//! (Function Code 0x14) and **Write File Record** (Function Code 0x15).
//!
//! File records are used to access large, structured memory areas that do not fit into
//! the standard coil or register addressing space. A single Modbus PDU can contain
//! multiple "sub-requests," each targeting a different file and record range.
//!
//! ## Key Features
//! - **Sub-Request Aggregation**: Manage multiple read or write operations in a single transaction.
//! - **PDU Size Validation**: Automatically calculates and validates byte counts to ensure
//!   compliance with the 253-byte Modbus limit.
//! - **no_std Compatible**: Uses `heapless::Vec` for data storage, avoiding dynamic allocation.
//!
//! ## Usage Example
//! ```rust
//! use mbus_core::models::file_record::SubRequest;
//!
//! let mut request = SubRequest::new();
//! // Add a request to read 10 registers from File 1, Record 0
//! request.add_read_sub_request(1, 0, 10).unwrap();
//! ```

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use heapless::Vec;

    /// Tests the creation of a new `SubRequest` and adding a read operation.
    #[test]
    fn test_add_read_sub_request() {
        let mut sub_req = SubRequest::new();
        // File 1, Record 2, Length 5
        let res = sub_req.add_read_sub_request(1, 2, 5);

        assert!(res.is_ok());
        // Header (7 bytes)
        assert_eq!(sub_req.byte_count(), 7);
    }

    /// Tests adding a write operation with data and verifies the byte count.
    #[test]
    fn test_add_write_sub_request() {
        let mut sub_req = SubRequest::new();
        let mut data = Vec::<u16, 252>::new();
        data.push(0x1234).unwrap();
        data.push(0x5678).unwrap();

        // File 1, Record 0, Length 2
        let res = sub_req.add_write_sub_request(1, 0, 2, data);

        assert!(res.is_ok());
        // Header (7) + Data (2 * 2) = 11 bytes
        assert_eq!(sub_req.byte_count(), 11);
    }

    /// Tests that the PDU overflow protection prevents adding too many sub-requests.
    #[test]
    fn test_pdu_overflow_protection() {
        let mut sub_req = SubRequest::new();

        // Add a very large read request
        let res = sub_req.add_read_sub_request(1, 0, 120);
        assert!(res.is_ok());

        // Adding another one should fail as it exceeds the 125 limit (SubReqs + Regs)
        let res_overflow = sub_req.add_read_sub_request(1, 120, 10);
        assert!(res_overflow.is_err());
    }

    /// Tests the conversion of sub-requests into raw PDU bytes.
    #[test]
    fn test_to_sub_req_pdu_bytes() {
        let mut sub_req = SubRequest::new();
        sub_req
            .add_read_sub_request(0x0001, 0x0002, 0x0003)
            .unwrap();

        let bytes = sub_req.to_sub_req_pdu_bytes().unwrap();

        // Expected: [ByteCount, RefType, FileHi, FileLo, RecHi, RecLo, LenHi, LenLo]
        // [0x07, 0x06, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03]
        let expected = [0x07, 0x06, 0x00, 0x01, 0x00, 0x02, 0x00, 0x03];
        assert_eq!(bytes.as_slice(), &expected);
    }

    /// Tests the clear functionality.
    #[test]
    fn test_clear_sub_requests() {
        let mut sub_req = SubRequest::new();
        sub_req.add_read_sub_request(1, 1, 1).unwrap();
        assert_eq!(sub_req.byte_count(), 7);

        sub_req.clear_all();
        assert_eq!(sub_req.byte_count(), 0);
    }
}
