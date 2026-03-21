use crate::errors::MbusError;

/// Maximum number of coils that can be read/written in a single Modbus PDU (2000 coils).
pub const MAX_COILS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the coil states for 2000 coils (250 bytes).
pub const MAX_COIL_BYTES: usize = (MAX_COILS_PER_PDU + 7) / 8; // 250 bytes for 2000 coils

/// Represents the state of a block of contiguous coils.
///
/// In the Modbus protocol, coils are 1-bit boolean values (ON = `true`, OFF = `false`) used to represent 
/// discrete outputs. To optimize network traffic and memory, these bits are tightly packed into 
/// bytes. This struct manages a specific continuous range of coils and abstracts away the complex 
/// bitwise operations required to get and set individual coil states.
///
/// # Examples
///
/// ```rust
/// use mbus_core::models::coil::{Coils, MAX_COIL_BYTES};
///
/// // Create a vector with 1 byte of data: 0b0000_0101 (Coils at offsets 0 and 2 are ON)
/// let mut values = [0x00; 1];
/// values[0] = 0x05;
///
/// // Initialize a block of 8 coils starting at Modbus address 100
/// let mut coils = Coils::new(100, 8);
/// coils.set_values(&values, 8).unwrap();
/// 
/// // Read a coil value (Address 100 corresponds to the first bit -> True)
/// assert_eq!(coils.value(100).unwrap(), true); // 0b0000_0101 -> LSB is 1 (true)
///
/// // Set a coil value using a base address and an offset (Base 100 + Offset 1 = 101)
/// coils.set_value(101, true).unwrap();
/// assert_eq!(coils.value(101).unwrap(), true);
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Coils {
    /// The starting address of the first coil in this block.
    from_address: u16,
    /// The number of coils in this block.
    quantity: u16,
    /// The coil states packed into bytes, where each bit represents a coil (1 for ON, 0 for OFF).
    values: [u8; MAX_COIL_BYTES], // Each bit represents a coil state
}

/// Provides operations for reading and writing Modbus coils.
impl Coils {
    /// Creates a new `Coils` instance representing a continuous block of coil states.
    ///
    /// # Arguments
    /// * `from_address` - The Modbus starting address for this block of coils.
    /// * `quantity` - The total number of consecutive coils managed by this instance.
    /// * `values` - The tightly bit-packed byte array representing the states of the coils.
    ///              The first byte represents coils `from_address` to `from_address + 7`,
    ///              where the LSB (Least Significant Bit) is `from_address`.
    ///
    /// # Returns
    /// A new initialized `Coils` instance.
    pub fn new(from_address: u16, quantity: u16) -> Self {
        Self {
            from_address,
            quantity,
            values: [0; MAX_COIL_BYTES],
        }
    }

    /// Sets the state of a specific coil within the block using a base address and an offset.
    ///
    /// This method calculates the target address by adding the `offset` to the provided `from_address`.
    /// It then validates that this target address falls within the range managed by this `Coils` instance.
    ///
    /// # Arguments
    /// * `from_address` - The base address to calculate from.
    /// * `offset` - The offset from the base address.
    /// * `value` - The boolean state to set (true for ON, false for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the value was successfully set, or `Err(MbusError::InvalidAddress)` if the
    /// calculated address is out of bounds.
    pub fn set_value(&mut self, address: u16, value: bool) -> Result<(), MbusError> {

        // Ensure the target address is within the range of this block
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }

        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;

        if value {
            self.values[byte_index] |= 1 << bit_in_byte; // Set bit to 1
        } else {
            self.values[byte_index] &= !(1 << bit_in_byte); // Set bit to 0
        }

        Ok(())
    }

    /// Updates the entire internal state of the coil block from a raw byte array.
    ///
    /// This method is typically used when a Modbus response is received and the raw
    /// bit-packed bytes need to be loaded into the `Coils` model.
    ///
    /// # Arguments
    /// * `values` - A reference to a fixed-size array of bytes containing the packed coil states.
    /// * `bits_length` - The number of valid bits (coils) represented in the `values` array.
    ///
    /// # Returns
    /// * `Ok(())` if the update was successful.
    /// * `Err(MbusError::InvalidQuantity)` if `bits_length` is less than the quantity managed by this instance.
    pub fn set_values(
        &mut self,
        values: &[u8],
        bits_length: u16,
    ) -> Result<(), MbusError> {
        if bits_length < self.quantity {
            return Err(MbusError::InvalidQuantity);
        }
        let byte_length = (bits_length + 7) / 8; // Round up to the nearest byte
        self.values[..byte_length as usize].copy_from_slice(&values[..byte_length as usize]);
        Ok(())
    }


    /// Returns the starting address of the first coil in this block.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the number of coils in this block.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns a reference to the array of bytes representing the coil states.
    pub fn values(&self) -> &[u8; MAX_COIL_BYTES] {
        &self.values
    }

    /// Retrieves the boolean state of a specific coil by its address.
    /// 
    /// # Arguments
    /// * `address` - The Modbus address of the coil to read.
    /// 
    /// # Returns
    /// `Ok(true)` if the coil is ON, `Ok(false)` if the coil is OFF, or `Err(MbusError::InvalidAddress)` if the address is out of bounds.
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
