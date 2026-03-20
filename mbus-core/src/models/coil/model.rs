use crate::errors::MbusError;
use heapless::Vec;

/// Maximum number of coils that can be read/written in a single Modbus PDU (2000 coils).
pub const MAX_COILS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the coil states for 2000 coils (250 bytes).
pub const MAX_COIL_BYTES: usize = (MAX_COILS_PER_PDU + 7) / 8; // 250 bytes for 2000 coils

/// Represents the state of a block of coils read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Coils {
    /// The starting address of the first coil in this block.
    from_address: u16,
    /// The number of coils in this block.
    quantity: u16,
    /// The coil states packed into bytes, where each bit represents a coil (1 for ON, 0 for OFF).
    values: Vec<u8, MAX_COIL_BYTES>, // Each bit represents a coil state
}

/// Provides operations for reading and writing Modbus coils.
impl Coils {
    /// Creates a new `Coils` instance with the given starting address, quantity, and coil states.
    pub fn new(from_address: u16, quantity: u16, values: Vec<u8, MAX_COIL_BYTES>) -> Self {
        Self {
            from_address,
            quantity,
            values,
        }
    }

    /// Returns the starting address of the first coil in this block.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the number of coils in this block.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns a reference to the vector of bytes representing the coil states.
    pub fn values(&self) -> &Vec<u8, MAX_COIL_BYTES> {
        &self.values
    }

    /// Retrieves the boolean state of a specific coil by its address.
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
