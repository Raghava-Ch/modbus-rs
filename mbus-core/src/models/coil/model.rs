use crate::errors::MbusError;

/// Maximum number of coils that can be read/written in a single Modbus PDU (2000 coils).
pub const MAX_COILS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the coil states for 2000 coils (250 bytes).
pub const MAX_COIL_BYTES: usize = MAX_COILS_PER_PDU.div_ceil(8); // 250 bytes for 2000 coils

/// Represents the state of a block of contiguous coils.
///
/// In the Modbus protocol, coils are 1-bit boolean values (ON = `true`, OFF = `false`) used to represent
/// discrete outputs. To optimize network traffic and memory, these bits are tightly packed into
/// bytes. This struct manages a specific continuous range of coils and abstracts away the complex
/// bitwise operations required to get and set individual coil states.
///
/// The `values` array stores these coil states. Each byte in `values` holds 8 coil states,
/// where the least significant bit (LSB) of the first byte corresponds to `from_address`,
/// the next bit to `from_address + 1`, and so on. `MAX_COIL_BYTES` is calculated to
/// accommodate `MAX_COILS_PER_PDU` coils (2000 coils require 250 bytes).
///
/// # Examples
///
/// ```rust
/// use mbus_core::models::coil::Coils;
/// use mbus_core::errors::MbusError;
///
/// // Initialize a block of 8 coils starting at Modbus address 100.
/// // Initially all coils are OFF (0).
/// let mut coils = Coils::new(100, 8).unwrap();
///
/// // Verify initial state: all coils are false
/// assert_eq!(coils.value(100).unwrap(), false);
/// assert_eq!(coils.value(107).unwrap(), false);
///
/// // Set coil at address 100 (offset 0) to ON
/// // Internal values: `values[0]` becomes `0b0000_0001`
/// coils.set_value(100, true).unwrap();
/// assert_eq!(coils.value(100).unwrap(), true);
/// assert_eq!(coils.values()[..1], [0b0000_0001]);
/// assert_eq!(coils.values()[..1], [0b0000_0001]);
/// 
/// // Set coil at address 102 (offset 2) to ON
/// // Internal values: `values[0]` becomes `0b0000_0101`
/// coils.set_value(102, true).unwrap();
/// assert_eq!(coils.value(102).unwrap(), true);
/// assert_eq!(coils.values()[..1], [0b0000_0101]);
/// assert_eq!(coils.values()[..1], [0b0000_0101]);
/// 
/// // Set coil at address 101 (offset 1) to ON
/// // Internal values: `values[0]` becomes `0b0000_0111`
/// coils.set_value(101, true).unwrap();
/// assert_eq!(coils.value(101).unwrap(), true);
/// assert_eq!(coils.values()[..1], [0b0000_0111]);
/// assert_eq!(coils.values()[..1], [0b0000_0111]);
/// 
/// // Set coil at address 100 back to OFF
/// // Internal values: `values[0]` becomes `0b0000_0110`
/// coils.set_value(100, false).unwrap();
/// assert_eq!(coils.value(100).unwrap(), false);
/// assert_eq!(coils.values()[..1], [0b0000_0110]);
/// assert_eq!(coils.values()[..1], [0b0000_0110]);
///
/// // Example with `with_values` for loading pre-packed data
/// let pre_packed_data = [0b1010_1010, 0b0101_0101]; // Two bytes for 16 coils
/// let mut loaded_coils = Coils::new(200, 16).unwrap()
///     .with_values(&pre_packed_data, 16)
///     .expect("Valid quantity and data");
///
/// assert_eq!(loaded_coils.value(200).unwrap(), false); // LSB of 0b1010_1010 is 0
/// assert_eq!(loaded_coils.value(201).unwrap(), true);  // Next bit is 1
/// assert_eq!(loaded_coils.value(208).unwrap(), true);  // LSB of 0b0101_0101 is 1 (first bit of second byte)
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Coils {
    /// The starting address of the first coil in this block.
    from_address: u16,
    /// The number of coils in this block.
    quantity: u16,
    /// The coil states packed into bytes, where each bit represents a coil (1 for ON, 0 for OFF). The least
    /// significant bit of `values[0]` corresponds to `from_address`.
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
    ///   The first byte represents coils `from_address` to `from_address + 7`,
    ///   where the LSB (Least Significant Bit) is `from_address`.
    ///
    /// # Returns
    /// A new initialized `Coils` instance.
    pub fn new(from_address: u16, quantity: u16) -> Result<Self, MbusError> {
        if quantity > MAX_COILS_PER_PDU as u16 {
            return Err(MbusError::InvalidQuantity);
        }
        Ok(Self {
            from_address,
            quantity,
            values: [0; MAX_COIL_BYTES],
        })
    }

    /// Sets the state of a specific coil within the block using a base address and an offset.
    ///
    /// This method calculates the target address by adding the `offset` to the provided `from_address`.
    /// It then validates that this target address falls within the range managed by this `Coils` instance.
    ///
    /// The coil's state is stored as a single bit within the `values` byte array.
    /// The `bit_index` is calculated as `address - self.from_address`.
    /// This `bit_index` is then used to determine the `byte_index` (`bit_index / 8`)
    /// and the `bit_in_byte` (`bit_index % 8`).
    ///
    /// To set a bit to `true` (ON), a bitwise OR operation (`|=`) is used with a mask `(1 << bit_in_byte)`.
    /// To set a bit to `false` (OFF), a bitwise AND NOT operation (`&= !(1 << bit_in_byte)`) is used.
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the coil to set.
    /// * `value` - The boolean state to set (`true` for ON, `false` for OFF).
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

    /// Sets the bit-packed values for the coils and validates the length.
    ///
    /// This method is typically used during the construction or update of a `Coils` model
    /// when a Modbus response is received. It ensures the provided data matches the
    /// expected quantity of coils.
    ///
    /// # Arguments
    /// * `values` - A slice of bytes containing the packed coil states.
    /// * `bits_length` - The number of bits (coils) actually contained in the provided values.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if the provided `bits_length` does not match
    /// the `quantity` initialized in the struct.
    pub fn with_values(mut self, values: &[u8], bits_length: u16) -> Result<Self, MbusError> {
        // Ensure we aren't receiving a different number of bits than the quantity we expect to manage
        if bits_length != self.quantity {
            return Err(MbusError::InvalidQuantity);
        }

        // Calculate how many bytes are needed to represent the bits_length (round up)
        let byte_length = bits_length.div_ceil(8);
        // Copy the relevant portion of the input slice into the internal fixed-size buffer
        self.values[..byte_length as usize].copy_from_slice(&values[..byte_length as usize]);
        Ok(self)
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
    /// This method calculates the `bit_index` as `address - self.from_address`.
    /// This `bit_index` is then used to determine the `byte_index` (`bit_index / 8`)
    /// within the `values` array and to create a `bit_mask` (`1u8 << (bit_index % 8)`)
    /// for the specific bit within that byte.
    ///
    /// A bitwise AND operation (`&`) with the `bit_mask` is performed on the relevant byte.
    /// If the result is non-zero, the bit is set (coil is ON); otherwise, it's OFF.
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
