//! # Modbus FIFO Queue Models
//!
//! This module defines the data structures for handling **Read FIFO Queue** (Function Code 0x18).
//!
//! In Modbus, a FIFO (First-In-First-Out) queue is a specialized structure where a set of
//! registers can be read from a single pointer address. When the client reads the FIFO
//! pointer, the server returns the current count of registers in the queue followed by
//! the register data itself.
//!
//! ## Key Components
//! - [`FifoQueue`]: A container for the registers retrieved from the FIFO.
//! - [`MAX_FIFO_QUEUE_COUNT_PER_PDU`]: The protocol limit for registers in one FIFO response.
//!
//! ## Protocol Limits
//! According to the Modbus specification, the FIFO count can range from 0 to 31 registers.
//! The response PDU includes a 2-byte byte count, a 2-byte FIFO count, and then the
//! register data (up to 62 bytes).

/// The maximum number of 16-bit registers that can be returned in a single Read FIFO Queue (FC 24) response.
///
/// The Modbus specification limits the FIFO count to 31 registers (62 bytes of data).
pub const MAX_FIFO_QUEUE_COUNT_PER_PDU: usize = 31;

/// A collection of register values retrieved from a Modbus FIFO queue.
///
/// This structure maintains the pointer address used for the request and stores the
/// resulting register values in a fixed-size array, making it suitable for `no_std`
/// and memory-constrained environments.
///
/// # Internal Representation
/// The `queue` field is a fixed-size array (`[u16; MAX_FIFO_QUEUE_COUNT_PER_PDU]`)
/// that stores the 16-bit register values. The `length` field tracks the actual
/// number of valid registers currently present in the `queue`, allowing the struct
/// to manage a variable number of registers within its fixed capacity.
///
/// # Examples
///
/// ```rust
/// use mbus_core::models::fifo_queue::FifoQueue;
/// use mbus_core::models::fifo_queue::MAX_FIFO_QUEUE_COUNT_PER_PDU;
///
/// // 1. Create a new FifoQueue instance for pointer address 0x1000.
/// // Initially, it's empty.
/// let mut fifo = FifoQueue::new(0x1000);
/// assert_eq!(fifo.ptr_address(), 0x1000);
/// assert_eq!(fifo.length(), 0);
/// assert!(fifo.queue().is_empty());
///
/// // 2. Simulate receiving a Modbus response with FIFO data.
/// // Let's say we read 3 registers: 0x1111, 0x2222, 0x3333.
/// // The `values` array would typically come from parsing the PDU.
/// let mut received_values = [0; MAX_FIFO_QUEUE_COUNT_PER_PDU];
/// received_values[0] = 0x1111;
/// received_values[1] = 0x2222;
/// received_values[2] = 0x3333;
/// let received_length = 3;
///
/// // 3. Populate the FifoQueue with the received data using `with_values`.
/// fifo = fifo.with_values(received_values, received_length);
///
/// // 4. Verify the contents and properties of the FIFO queue.
/// assert_eq!(fifo.length(), 3);
/// assert_eq!(fifo.queue()[..3], [0x1111, 0x2222, 0x3333]);
/// assert_eq!(fifo.queue()[..1], [0x1111]);
/// assert_eq!(fifo.queue()[1..2], [0x2222]);
/// assert_eq!(fifo.queue()[2..3], [0x3333]);
/// ```
#[derive(Debug, Clone)]
pub struct FifoQueue {
    /// The Modbus address of the FIFO pointer.
    ptr_address: u16,
    /// The register values read from the FIFO, stored as 16-bit unsigned integers.
    queue: [u16; MAX_FIFO_QUEUE_COUNT_PER_PDU],
    /// The actual number of valid registers currently stored in the `values` array.
    length: usize,
}

impl FifoQueue {
    /// Creates a new `FifoQueue` instance.
    ///
    /// # What happens:
    /// A new `FifoQueue` is created with the specified `ptr_address`.
    /// The internal `queue` array is initialized to all zeros, and the `length`
    /// is set to 0, indicating an empty queue.
    ///
    /// # Arguments
    /// * `ptr_address` - The Modbus address of the FIFO pointer that was queried.
    pub fn new(ptr_address: u16) -> Self {
        Self {
            ptr_address,
            // Initialize with zeros; the actual data will be loaded via `with_values`
            queue: [0; MAX_FIFO_QUEUE_COUNT_PER_PDU],
            length: 0,
        }
    }

    /// Returns the Modbus address of the FIFO pointer.
    ///
    /// This is the address that was provided in the original Read FIFO Queue request.
    ///
    /// # Returns
    /// The `u16` Modbus address of the FIFO pointer.
    pub fn ptr_address(&self) -> u16 {
        self.ptr_address
    }

    /// Returns a reference to the active values in the FIFO queue.
    ///
    /// This method provides a slice `&[u16]` containing only the `length`
    /// valid registers, effectively hiding the unused capacity of the
    /// internal fixed-size array.
    ///
    /// # Returns
    /// A slice `&[u16]` of the register values.
    pub fn queue(&self) -> &[u16] {
        &self.queue[..self.length]
    }

    /// Returns the number of registers currently held in this FIFO response.
    ///
    /// # Returns
    /// The `usize` number of registers in the queue.
    pub fn length(&self) -> usize {
        self.length
    }

    /// Loads the register values into the FIFO model and sets the active length.
    ///
    /// This method is typically used by the service layer after parsing a Modbus response PDU.
    ///
    /// # Arguments
    /// * `values` - A fixed-size array containing the 16-bit register values.
    ///   This array should have a length of `MAX_FIFO_QUEUE_COUNT_PER_PDU`.
    /// * `length` - The number of registers to be considered active.
    ///
    /// # What happens:
    /// 1. The entire `values` array is copied into the internal `self.queue`.
    /// 2. The `length` is set, but it is clamped to `MAX_FIFO_QUEUE_COUNT_PER_PDU`
    ///    to prevent exceeding the buffer's capacity.
    ///
    /// # Returns
    /// The updated `FifoQueue` instance.
    pub fn with_values(
        mut self,
        values: [u16; MAX_FIFO_QUEUE_COUNT_PER_PDU],
        length: usize,
    ) -> Self {
        self.queue = values;
        self.length = core::cmp::min(length, MAX_FIFO_QUEUE_COUNT_PER_PDU);
        self
    }
}
