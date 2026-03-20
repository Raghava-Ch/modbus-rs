use heapless::Vec;

use crate::errors::MbusError;

/// Maximum number of discrete inputs that can be read in a single Modbus PDU (2000 inputs).
pub const MAX_DISCRETE_INPUTS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the input states for 2000 inputs (250 bytes).
pub const MAX_DISCRETE_INPUT_BYTES: usize = (MAX_DISCRETE_INPUTS_PER_PDU + 7) / 8;

/// Represents the state of a block of discrete inputs read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DiscreteInputs {
    /// The starting address of the first input in this block.
    from_address: u16,
    /// The number of inputs in this block.
    quantity: u16,
    /// The input states packed into bytes, where each bit represents an input (1 for ON, 0 for OFF).
    values: Vec<u8, MAX_DISCRETE_INPUT_BYTES>,
}

impl DiscreteInputs {
    /// Creates a new `DiscreteInputs` instance.
    pub fn new(
        from_address: u16,
        quantity: u16,
        values: Vec<u8, MAX_DISCRETE_INPUT_BYTES>,
    ) -> Self {
        Self {
            from_address,
            quantity,
            values,
        }
    }

    /// Returns the starting address.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the quantity of inputs.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns the input values as bytes.
    pub fn values(&self) -> &Vec<u8, MAX_DISCRETE_INPUT_BYTES> {
        &self.values
    }

    /// Retrieves the boolean state of a specific input by its address.
    pub fn value(&self, address: u16) -> Result<bool, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_mask = 1u8 << (bit_index % 8);

        Ok(self.values[byte_index] & bit_mask != 0)
    }
}
