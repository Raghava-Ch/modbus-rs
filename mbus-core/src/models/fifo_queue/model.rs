use heapless::Vec;

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
