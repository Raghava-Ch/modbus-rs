//! # Modbus FIFO Queue Models
//!
//! This module provides the data structures and logic for handling **Read FIFO Queue**
//! (Function Code 0x18).
//!
//! A FIFO (First-In-First-Out) queue in Modbus is a specialized structure where a set of
//! registers can be read from a single pointer address. This is often used for
//! data logging or buffering where multiple data points are collected and read in bulk.
//!
//! ## Key Features
//! - **Fixed-Size Storage**: Uses a fixed-size array to store up to 31 registers,
//!   aligning with the Modbus protocol limits.
//! - **no_std Compatible**: Designed for embedded systems without heap allocation.
//! - **Safe Access**: Provides a clean API to retrieve the pointer address and the
//!   sequence of register values.

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Tests the creation of a new `FifoQueue` and verifies its initial state.
    #[test]
    fn test_fifo_queue_new() {
        let ptr_addr = 0x04D2; // 1234
        let fifo = FifoQueue::new(ptr_addr);

        assert_eq!(fifo.ptr_address(), ptr_addr);
        assert_eq!(fifo.length(), 0);
        assert_eq!(fifo.queue().len(), 0);
    }

    /// Tests loading values into the FIFO queue and verifying the data integrity.
    #[test]
    fn test_fifo_queue_with_values() {
        let mut values = [0u16; MAX_FIFO_QUEUE_COUNT_PER_PDU];
        values[0] = 0xAAAA;
        values[1] = 0xBBBB;
        values[2] = 0xCCCC;

        let fifo = FifoQueue::new(100).with_values(values, 3);

        assert_eq!(fifo.length(), 3);
        assert_eq!(fifo.queue(), &values[..3]);
        assert_eq!(fifo.queue()[0], 0xAAAA);
    }

    /// Tests that the FIFO queue correctly handles and clamps lengths exceeding the PDU limit.
    #[test]
    fn test_fifo_queue_overflow_protection() {
        let values = [1u16; MAX_FIFO_QUEUE_COUNT_PER_PDU];

        // Attempt to set a length of 50, which exceeds the protocol limit of 31
        let fifo = FifoQueue::new(100).with_values(values, 50);

        // The internal logic should clamp the length to MAX_FIFO_QUEUE_COUNT_PER_PDU
        assert_eq!(fifo.length(), MAX_FIFO_QUEUE_COUNT_PER_PDU);
        assert_eq!(fifo.queue().len(), MAX_FIFO_QUEUE_COUNT_PER_PDU);
    }
}
