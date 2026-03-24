//! # Modbus Discrete Input Models
//!
//! This module defines the data structures for handling **Discrete Inputs** (Function Code 0x02).
//!
//! In Modbus, Discrete Inputs are single-bit, read-only data objects. They are typically used
//! to represent digital inputs from physical devices, such as limit switches, sensor states,
//! or status indicators.
//!
//! ## Key Components
//! - [`DiscreteInputs`]: A container for a block of bit-packed input states.
//! - [`MAX_DISCRETE_INPUTS_PER_PDU`]: The protocol limit for a single read operation.
//!
//! ## Data Packing¯
//! Discrete inputs are packed into bytes in the Modbus PDU. The first input requested
//! is stored in the Least Significant Bit (LSB) of the first data byte.
//!
//! ### Example
//! If 3 inputs are read (Address 10, 11, 12) and the first and third are ON:
//! - Byte 0: `0000 0101` (Binary) -> `0x05` (Hex)
//!   - Bit 0 (Address 10): 1 (ON)
//!   - Bit 1 (Address 11): 0 (OFF)
//!   - Bit 2 (Address 12): 1 (ON)

use crate::errors::MbusError;

/// The maximum number of discrete inputs that can be requested in a single Read Discrete Inputs (FC 02) PDU.
///
/// According to the Modbus Application Protocol Specification V1.1b3, the quantity of inputs
/// must be between 1 and 2000 (0x07D0).
pub const MAX_DISCRETE_INPUTS_PER_PDU: usize = 2000;

/// The maximum number of bytes required to store the bit-packed states of 2000 discrete inputs.
///
/// Calculated as `ceil(2000 / 8) = 250` bytes.
pub const MAX_DISCRETE_INPUT_BYTES: usize = MAX_DISCRETE_INPUTS_PER_PDU.div_ceil(8);

/// A collection of discrete input states retrieved from a Modbus server.
///
/// This structure maintains the context of the read operation (starting address and quantity)
/// and stores the actual bit-packed values in a memory-efficient `heapless::Vec`, making it
/// suitable for `no_std` and embedded environments.
///
/// Use the [`value()`](Self::value) method to extract individual boolean states without
/// manually performing bitwise operations.
///
/// # Internal Representation
/// The `values` array stores these discrete input states. Each byte in `values` holds 8 input states,
/// where the least significant bit (LSB) of the first byte (`values[0]`) corresponds to the
/// `from_address`, the next bit to `from_address + 1`, and so on. This bit-packing is efficient
/// for memory usage and network transmission.
///
/// The `MAX_DISCRETE_INPUT_BYTES` constant ensures that the `values` array has enough space to
/// accommodate the maximum possible number of discrete inputs allowed in a single Modbus PDU
/// (`MAX_DISCRETE_INPUTS_PER_PDU`).
///
/// # Examples
///
/// ```rust
/// use mbus_core::models::discrete_input::{DiscreteInputs, MAX_DISCRETE_INPUT_BYTES};
/// use mbus_core::errors::MbusError;
///
/// // Initialize a block of 8 discrete inputs starting at Modbus address 100.
/// // Initially all inputs are OFF (0).
/// let mut inputs = DiscreteInputs::new(100, 8).unwrap();
///
/// // Verify initial state: all inputs are false
/// assert_eq!(inputs.value(100).unwrap(), false);
/// assert_eq!(inputs.value(107).unwrap(), false);
///
/// // Simulate receiving data where inputs at offsets 0 and 2 are ON (0b0000_0101)
/// let received_data = [0x05, 0x00, 0x00, 0x00]; // Only the first byte is relevant for 8 inputs
/// inputs = inputs.with_values(&received_data, 8).expect("Valid quantity and data");
///
/// // Read individual input values
/// assert_eq!(inputs.value(100).unwrap(), true);  // Address 100 (offset 0) -> LSB of 0x05 is 1
/// assert_eq!(inputs.value(101).unwrap(), false); // Address 101 (offset 1) -> next bit is 0
/// assert_eq!(inputs.value(102).unwrap(), true);  // Address 102 (offset 2) -> next bit is 1
/// assert_eq!(inputs.value(107).unwrap(), false); // Address 107 (offset 7) -> MSB of 0x05 is 0
///
/// // Accessing values out of bounds will return an error
/// assert_eq!(inputs.value(99), Err(MbusError::InvalidAddress));
/// assert_eq!(inputs.value(108), Err(MbusError::InvalidAddress));
///
/// // Get the raw bit-packed bytes (only the first byte is active for 8 inputs)
/// assert_eq!(inputs.values(), &[0x05]);
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DiscreteInputs {
    /// The starting address of the first input in this block.
    from_address: u16,
    /// The number of inputs in this block.
    quantity: u16,
    /// The input states packed into bytes, where each bit represents an input (1 for ON, 0 for OFF).
    /// The least significant bit of `values[0]` corresponds to `from_address`.
    values: [u8; MAX_DISCRETE_INPUT_BYTES],
}

impl DiscreteInputs {
    /// Creates a new `DiscreteInputs` instance representing a block of read-only discrete inputs.
    ///
    /// The internal `values` array is initialized to all zeros, meaning all discrete inputs
    /// are initially considered OFF (`false`).
    ///
    /// # Arguments
    /// * `from_address` - The starting Modbus address for this block of inputs.
    /// * `quantity` - The total number of discrete inputs contained in this block.
    ///
    /// # What happens:
    /// 1. The `quantity` is validated to ensure it does not exceed `MAX_DISCRETE_INPUTS_PER_PDU`.
    /// 2. A new `DiscreteInputs` instance is created with the specified `from_address` and `quantity`.
    /// 3. The internal `values` array, which stores the bit-packed states, is initialized to all `0u8`s.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if the requested `quantity` exceeds
    /// `MAX_DISCRETE_INPUTS_PER_PDU`.
    /// # Returns
    /// A new initialized `DiscreteInputs` instance.
    pub fn new(from_address: u16, quantity: u16) -> Result<Self, MbusError> {
        if quantity > MAX_DISCRETE_INPUTS_PER_PDU as u16 {
            return Err(MbusError::InvalidQuantity);
        }
        Ok(Self {
            from_address,
            quantity,
            values: [0; MAX_DISCRETE_INPUT_BYTES],
        })
    }

    /// Sets the bit-packed values for the discrete inputs and validates the length.
    ///
    /// This method is typically used to populate a `DiscreteInputs` instance with actual
    /// data received from a Modbus server. It copies the relevant portion of the provided
    /// `values` slice into the internal fixed-size buffer.
    ///
    /// # Arguments
    /// * `values` - A slice of bytes containing the bit-packed states. This slice should
    ///   be at least as long as the number of bytes required to store `bits_length` inputs.
    /// * `bits_length` - The number of bits (inputs) actually contained in the provided values.
    ///   This parameter specifies the *actual* number of bits (discrete inputs) present in
    ///   the `values` slice, which should typically match the `quantity` of the
    ///   `DiscreteInputs` instance.
    ///
    /// # What happens:
    /// 1. The `bits_length` is checked against the `quantity` of the `DiscreteInputs` instance.
    /// 2. The necessary number of bytes (`byte_length`) is calculated from `bits_length`.
    /// 3. The relevant portion of the input `values` slice is copied into the internal `self.values` array.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if `bits_length` does not match `self.quantity`.
    pub fn with_values(mut self, values: &[u8], bits_length: u16) -> Result<Self, MbusError> {
        if bits_length > self.quantity {
            return Err(MbusError::InvalidQuantity);
        }

        // Ensure we aren't receiving fewer bits than the quantity we expect to manage
        if bits_length < self.quantity {
            return Err(MbusError::InvalidQuantity);
        }

        // Calculate how many bytes are needed to represent the bits_length (round up)
        let byte_length = bits_length.div_ceil(8);
        // Copy only the relevant portion of the input array into the internal buffer
        self.values[..byte_length as usize].copy_from_slice(&values[..byte_length as usize]);
        Ok(self)
    }

    /// Returns the starting Modbus address of the first discrete input in this block.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the total number of discrete inputs managed by this instance.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns a reference to the active bytes containing the bit-packed input states.
    ///
    /// This method returns a slice `&[u8]` that contains only the bytes relevant to the
    /// `quantity` of discrete inputs managed by this instance. It does not return the
    /// entire `MAX_DISCRETE_INPUT_BYTES` array if `quantity` is smaller.
    /// The length of the returned slice is calculated as `ceil(self.quantity / 8)`.
    ///
    pub fn values(&self) -> &[u8] {
        let byte_length = (self.quantity as usize).div_ceil(8);
        &self.values[..byte_length]
    }

    /// Retrieves the boolean state of a specific input by its address.
    ///
    /// This method performs boundary checking to ensure the requested address is within
    /// the range [from_address, from_address + quantity).
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the discrete input to query.
    ///
    /// # What happens:
    /// 1. **Boundary Check**: The `address` is validated to ensure it falls within the range
    ///    `[self.from_address, self.from_address + self.quantity)`.
    /// 2. **Bit Index Calculation**: The `bit_index` (zero-based offset from `from_address`) is calculated.
    /// 3. **Byte and Bit Position**: The `byte_index` (`bit_index / 8`) determines which byte in the
    ///    `values` array contains the target bit, and `bit_in_byte` (`bit_index % 8`) determines
    ///    its position within that byte.
    /// 4. **Masking**: A `bit_mask` (e.g., `0b0000_0001` for bit 0, `0b0000_0010` for bit 1) is created
    ///    to isolate the specific bit.
    /// 5. **Extraction**: A bitwise AND operation (`&`) with the `bit_mask` is performed on the relevant byte.
    ///    If the result is non-zero, the bit is ON (`true`); otherwise, it's OFF (`false`).
    ///
    /// # Returns
    /// * `Ok(true)` if the input is ON (1).
    /// * `Ok(false)` if the input is OFF (0).
    /// * `Err(MbusError::InvalidAddress)` if the address is out of the block's range.
    pub fn value(&self, address: u16) -> Result<bool, MbusError> {
        // Check if the requested address falls within our managed range
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }

        // Calculate which byte and which bit within that byte contains the state
        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_mask = 1u8 << (bit_index % 8);

        // Extract the bit using the mask and convert to boolean
        Ok(self.values[byte_index] & bit_mask != 0)
    }
}
