use crate::errors::MbusError;

/// Maximum number of coils that can be read/written in a single Modbus PDU (2000 coils).
pub const MAX_COILS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the coil states for 2000 coils (250 bytes).
pub const MAX_COIL_BYTES: usize = MAX_COILS_PER_PDU.div_ceil(8); // 250 bytes for 2000 coils

/// Represents the state of a single Modbus coil (On or Off).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoilState {
    /// Coil is active / energised / high (1).
    On,
    /// Coil is inactive / de-energised / low (0).
    Off,
}

impl CoilState {
    /// Returns `true` if the coil state is [`CoilState::On`].
    #[inline]
    pub fn to_bit(&self) -> u8 {
        match self {
            CoilState::On => 1,
            CoilState::Off => 0,
        }
    }

    /// Converts a 16-bit raw Modbus coil representation (`0xFF00` or `0x0000`) into a [`CoilState`].
    #[inline]
    pub fn from_u16(raw: u16) -> Self {
        if crate::data_unit::common::is_coil_on(raw) {
            CoilState::On
        } else {
            CoilState::Off
        }
    }

    /// Converts this [`CoilState`] into its 16-bit raw Modbus coil representation (`0xFF00` for `On`, `0x0000` for `Off`).
    #[inline]
    pub fn to_u16(&self) -> u16 {
        match self {
            CoilState::On => crate::data_unit::common::COIL_VALUE_ON,
            CoilState::Off => crate::data_unit::common::COIL_VALUE_OFF,
        }
    }
}

#[cfg(feature = "error-trait")]
impl core::fmt::Display for CoilState {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CoilState::On => write!(f, "ON"),
            CoilState::Off => write!(f, "OFF"),
        }
    }
}

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
/// use mbus_core::models::coil::{Coils, CoilState};
/// use mbus_core::errors::MbusError;
///
/// // Initialize a block of 8 coils starting at Modbus address 100.
/// // Initially all coils are OFF (0).
/// let mut coils = Coils::new(100, 8).unwrap();
///
/// // Verify initial state: all coils are false / Off
/// assert_eq!(coils.value(100).unwrap(), CoilState::Off);
/// assert_eq!(coils.value(107).unwrap(), CoilState::Off);
///
/// // Set coil at address 100 (offset 0) to ON
/// coils.set_value(100, CoilState::On).unwrap();
/// assert_eq!(coils.value(100).unwrap(), CoilState::On);
/// assert_eq!(coils.values()[..1], [0b0000_0001]);
///
/// // Set coil at address 102 (offset 2) to ON
/// coils.set_value(102, CoilState::On).unwrap();
/// assert_eq!(coils.value(102).unwrap(), CoilState::On);
/// assert_eq!(coils.values()[..1], [0b0000_0101]);
///
/// // Set coil at address 100 back to OFF
/// coils.set_value(100, CoilState::Off).unwrap();
/// assert_eq!(coils.value(100).unwrap(), CoilState::Off);
/// assert_eq!(coils.values()[..1], [0b0000_0100]);
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
    /// Validates `quantity` using [`crate::data_unit::common::validate_quantity`].
    ///
    /// # Arguments
    /// * `from_address` - The Modbus starting address for this block of coils.
    /// * `quantity` - The total number of consecutive coils managed by this instance.
    ///
    /// # Returns
    /// A new initialized `Coils` instance, or an `Err(MbusError::InvalidQuantity)` if quantity is 0 or exceeds limits.
    pub fn new(from_address: u16, quantity: u16) -> Result<Self, MbusError> {
        crate::data_unit::common::validate_quantity(
            crate::function_codes::public::FunctionCode::ReadCoils,
            quantity,
        )?;
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
    /// To set a bit to [`CoilState::On`], a bitwise OR operation (`|=`) is used with a mask `(1 << bit_in_byte)`.
    /// To set a bit to [`CoilState::Off`], a bitwise AND NOT operation (`&= !(1 << bit_in_byte)`) is used.
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the coil to set.
    /// * `value` - The state to set (accepts [`CoilState`], `true` for ON, `false` for OFF).
    ///
    /// # Returns
    /// `Ok(())` if the value was successfully set, or `Err(MbusError::InvalidAddress)` if the
    /// calculated address is out of bounds.
    pub fn set_value(&mut self, address: u16, state: CoilState) -> Result<(), MbusError> {
        // Ensure the target address is within the range of this block
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }

        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_in_byte = bit_index % 8;

        if state == CoilState::On {
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
    pub fn with_raw_values(
        mut self,
        raw_values: &[u8],
        bits_length: u16,
    ) -> Result<Self, MbusError> {
        // Ensure we aren't receiving a different number of bits than the quantity we expect to manage
        if bits_length != self.quantity {
            return Err(MbusError::InvalidQuantity);
        }

        // Calculate how many bytes are needed to represent the bits_length (round up)
        let byte_length = bits_length.div_ceil(8);
        // Copy the relevant portion of the input slice into the internal fixed-size buffer
        self.values[..byte_length as usize].copy_from_slice(&raw_values[..byte_length as usize]);
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
    pub fn raw_values(&self) -> &[u8; MAX_COIL_BYTES] {
        &self.values
    }

    /// Retrieves the [`CoilState`] of a specific coil by its address.
    ///
    /// This method calculates the `bit_index` as `address - self.from_address`.
    /// This `bit_index` is then used to determine the `byte_index` (`bit_index / 8`)
    /// within the `values` array and to create a `bit_mask` (`1u8 << (bit_index % 8)`)
    /// for the specific bit within that byte.
    ///
    /// A bitwise AND operation (`&`) with the `bit_mask` is performed on the relevant byte.
    /// If the result is non-zero, the bit is set ([`CoilState::On`]); otherwise, it's [`CoilState::Off`].
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the coil to read.
    ///
    /// # Returns
    /// `Ok(CoilState::On)` if the coil is ON, `Ok(CoilState::Off)` if the coil is OFF, or `Err(MbusError::InvalidAddress)` if the address is out of bounds.
    pub fn value(&self, address: u16) -> Result<CoilState, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_mask = 1u8 << (bit_index % 8);

        if self.values[byte_index] & bit_mask != 0 {
            Ok(CoilState::On)
        } else {
            Ok(CoilState::Off)
        }
    }
}
