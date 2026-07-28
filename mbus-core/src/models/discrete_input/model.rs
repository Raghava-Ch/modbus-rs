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
//! - [`DiscreteInputState`]: Represents the state of a discrete input ([`DiscreteInputState::On`] or [`DiscreteInputState::Off`]).
//! - [`MAX_DISCRETE_INPUTS_PER_PDU`]: The protocol limit for a single read operation (2000 inputs).
//! - [`MAX_DISCRETE_INPUT_BYTES`]: The maximum bytes required to store 2000 packed inputs (250 bytes).
//!
//! ## Data Packing
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
use crate::models::coil::{Coils, CoilState};

/// The maximum number of bytes required to store the bit-packed states of 2000 discrete inputs.
///
/// Calculated as `ceil(2000 / 8) = 250` bytes.
pub use crate::models::coil::MAX_COIL_BYTES as MAX_DISCRETE_INPUT_BYTES;

/// The maximum number of discrete inputs that can be requested in a single Read Discrete Inputs (FC 02) PDU.
///
/// According to the Modbus Application Protocol Specification V1.1b3, the quantity of inputs
/// must be between 1 and 2000 (0x07D0).
pub use crate::models::coil::MAX_COILS_PER_PDU as MAX_DISCRETE_INPUTS_PER_PDU;

/// Represents the state of a single Modbus discrete input ([`DiscreteInputState::On`] or [`DiscreteInputState::Off`]).
///
/// Shared bit semantics with [`CoilState`].
pub type DiscreteInputState = CoilState;

/// A collection of discrete input states retrieved from a Modbus server.
///
/// This structure maintains the context of the read operation (starting address and quantity)
/// and wraps [`Coils`] as a read-only container for discrete inputs (Function Code 0x02).
///
/// Use the [`value()`](Self::value) method to extract individual [`DiscreteInputState`] values
/// without manually performing bitwise operations.
///
/// # Internal Representation
/// The internal buffer stores these discrete input states. Each byte holds 8 input states,
/// where the least significant bit (LSB) of the first byte corresponds to the `from_address`,
/// the next bit to `from_address + 1`, and so on.
///
/// The `MAX_DISCRETE_INPUT_BYTES` constant ensures that the internal buffer has enough space to
/// accommodate the maximum possible number of discrete inputs allowed in a single Modbus PDU
/// (`MAX_DISCRETE_INPUTS_PER_PDU`).
///
/// # Examples
///
/// ```rust
/// use mbus_core::models::discrete_input::{DiscreteInputs, DiscreteInputState, MAX_DISCRETE_INPUT_BYTES};
/// use mbus_core::errors::MbusError;
///
/// // Initialize a block of 8 discrete inputs starting at Modbus address 100.
/// // Initially all inputs are OFF.
/// let mut inputs = DiscreteInputs::new(100, 8).unwrap();
///
/// // Verify initial state: all inputs are OFF
/// assert_eq!(inputs.value(100).unwrap(), DiscreteInputState::Off);
/// assert_eq!(inputs.value(107).unwrap(), DiscreteInputState::Off);
///
/// // Simulate receiving data where inputs at offsets 0 and 2 are ON (0b0000_0101)
/// let received_data = [0x05, 0x00, 0x00, 0x00];
/// inputs = inputs.with_values(&received_data, 8).expect("Valid quantity and data");
///
/// // Read individual input values
/// assert_eq!(inputs.value(100).unwrap(), DiscreteInputState::On);   // Address 100 (offset 0) -> LSB of 0x05 is 1
/// assert_eq!(inputs.value(101).unwrap(), DiscreteInputState::Off);  // Address 101 (offset 1) -> next bit is 0
/// assert_eq!(inputs.value(102).unwrap(), DiscreteInputState::On);   // Address 102 (offset 2) -> next bit is 1
/// assert_eq!(inputs.value(107).unwrap(), DiscreteInputState::Off);  // Address 107 (offset 7) -> MSB of 0x05 is 0
///
/// // Accessing values out of bounds will return an error
/// assert_eq!(inputs.value(99), Err(MbusError::InvalidAddress));
/// assert_eq!(inputs.value(108), Err(MbusError::InvalidAddress));
///
/// // Get the raw bit-packed bytes (only the active byte is returned)
/// assert_eq!(inputs.values(), &[0x05]);
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DiscreteInputs(Coils);

impl DiscreteInputs {
    /// Creates a new `DiscreteInputs` instance representing a block of read-only discrete inputs.
    ///
    /// The internal buffer is initialized to all zeros, meaning all discrete inputs
    /// are initially considered OFF ([`DiscreteInputState::Off`]).
    ///
    /// # Arguments
    /// * `from_address` - The starting Modbus address for this block of inputs.
    /// * `quantity` - The total number of discrete inputs contained in this block.
    ///
    /// # What happens:
    /// 1. The `quantity` is validated to ensure it does not exceed `MAX_DISCRETE_INPUTS_PER_PDU`.
    /// 2. A new `DiscreteInputs` instance is created wrapping an initialized [`Coils`] struct.
    /// 3. The internal buffer storing the bit-packed states is set to zero bytes.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if the requested `quantity` is 0 or exceeds
    /// `MAX_DISCRETE_INPUTS_PER_PDU`.
    ///
    /// # Returns
    /// A new initialized `DiscreteInputs` instance.
    pub fn new(from_address: u16, quantity: u16) -> Result<Self, MbusError> {
        Coils::new(from_address, quantity).map(Self)
    }

    /// Sets the bit-packed values for the discrete inputs using raw byte values and validates length.
    ///
    /// This method is typically used to populate a `DiscreteInputs` instance with actual
    /// data received from a Modbus server. It copies the relevant portion of the provided
    /// `raw_values` slice into the internal fixed-size buffer.
    ///
    /// # Arguments
    /// * `raw_values` - A slice of bytes containing the bit-packed states.
    /// * `bits_length` - The number of bits (inputs) contained in the provided values.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if `bits_length` does not match `self.quantity()`.
    pub fn with_raw_values(self, raw_values: &[u8], bits_length: u16) -> Result<Self, MbusError> {
        self.0.with_raw_values(raw_values, bits_length).map(Self)
    }

    /// Sets the bit-packed values for the discrete inputs and validates the length.
    ///
    /// Compatibility helper wrapping [`with_raw_values`](Self::with_raw_values).
    ///
    /// # Arguments
    /// * `values` - A slice of bytes containing the bit-packed states.
    /// * `bits_length` - The number of bits (inputs) contained in the provided values.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidQuantity` if `bits_length` does not match `self.quantity()`.
    pub fn with_values(self, values: &[u8], bits_length: u16) -> Result<Self, MbusError> {
        self.with_raw_values(values, bits_length)
    }

    /// Returns the starting Modbus address of the first discrete input in this block.
    pub fn from_address(&self) -> u16 {
        self.0.from_address()
    }

    /// Returns the total number of discrete inputs managed by this instance.
    pub fn quantity(&self) -> u16 {
        self.0.quantity()
    }

    /// Returns a reference to the fixed-size array of raw bytes.
    pub fn raw_values(&self) -> &[u8; MAX_DISCRETE_INPUT_BYTES] {
        self.0.raw_values()
    }

    /// Returns a reference to the active bytes containing the bit-packed input states.
    ///
    /// Returns a slice `&[u8]` containing only the bytes relevant to the `quantity`
    /// managed by this instance. The length of the returned slice is calculated as `ceil(self.quantity() / 8)`.
    pub fn values(&self) -> &[u8] {
        let byte_length = (self.quantity() as usize).div_ceil(8);
        &self.raw_values()[..byte_length]
    }

    /// Retrieves the [`DiscreteInputState`] of a specific input by its address.
    ///
    /// This method performs boundary checking to ensure the requested address is within
    /// the range `[from_address, from_address + quantity)`.
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the discrete input to query.
    ///
    /// # What happens:
    /// 1. **Boundary Check**: Validates `address` falls within `[self.from_address(), self.from_address() + self.quantity())`.
    /// 2. **Bit Extraction**: Locates target byte and bit mask within the internal bit-packed representation.
    /// 3. **State Mapping**: Returns [`DiscreteInputState::On`] if bit is 1, or [`DiscreteInputState::Off`] if bit is 0.
    ///
    /// # Returns
    /// * `Ok(DiscreteInputState::On)` if the input is active (1).
    /// * `Ok(DiscreteInputState::Off)` if the input is inactive (0).
    /// * `Err(MbusError::InvalidAddress)` if `address` is out of bounds.
    pub fn value(&self, address: u16) -> Result<DiscreteInputState, MbusError> {
        self.0.value(address)
    }
}
