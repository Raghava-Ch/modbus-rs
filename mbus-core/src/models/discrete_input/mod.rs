//! # Modbus Discrete Input Models
//!
//! This module provides the data structures and logic for handling **Discrete Inputs**
//! (Function Code 0x02).
//!
//! Discrete Inputs are single-bit, read-only data objects typically used to represent
//! digital inputs from physical devices, such as limit switches or sensor states.
//!
//! ## Key Features
//! - **Memory Efficient**: Uses bit-packing to store up to 2000 inputs in a fixed-size buffer.
//! - **no_std Compatible**: Designed for embedded systems without heap allocation.
//! - **Safe Access**: Provides methods to retrieve individual bit states by their Modbus address
//!   with automatic boundary checking.
//! 
//! A collection of discrete input states retrieved from a Modbus server.
//!
//! This structure maintains the context of the read operation (starting address and quantity)
//! and stores the actual bit-packed values in a memory-efficient `heapless::Vec`, making it
//! suitable for `no_std` and embedded environments.
//!
//! Use the [`value()`](Self::value) method to extract individual boolean states without
//! manually performing bitwise operations.
//!
//! # Internal Representation
//! The `values` array stores these discrete input states. Each byte in `values` holds 8 input states,
//! where the least significant bit (LSB) of the first byte (`values[0]`) corresponds to the
//! `from_address`, the next bit to `from_address + 1`, and so on. This bit-packing is efficient
//! for memory usage and network transmission.
//!
//! The `MAX_DISCRETE_INPUT_BYTES` constant ensures that the `values` array has enough space to
//! accommodate the maximum possible number of discrete inputs allowed in a single Modbus PDU
//! (`MAX_DISCRETE_INPUTS_PER_PDU`).
//!
//! # Examples
//!
//! ```rust
//! use mbus_core::models::discrete_input::{DiscreteInputs, MAX_DISCRETE_INPUT_BYTES};
//! use mbus_core::errors::MbusError;
//!
//! // Initialize a block of 8 discrete inputs starting at Modbus address 100.
//! // Initially all inputs are OFF (0).
//! let mut inputs = DiscreteInputs::new(100, 8).unwrap();
//!
//! // Verify initial state: all inputs are false
//! assert_eq!(inputs.value(100).unwrap(), false);
//! assert_eq!(inputs.value(107).unwrap(), false);
//!
//! // Simulate receiving data where inputs at offsets 0 and 2 are ON (0b0000_0101)
//! let received_data = [0x05, 0x00, 0x00, 0x00]; // Only the first byte is relevant for 8 inputs
//! inputs = inputs.with_values(&received_data, 8).expect("Valid quantity and data");
//!
//! // Read individual input values
//! assert_eq!(inputs.value(100).unwrap(), true);  // Address 100 (offset 0) -> LSB of 0x05 is 1
//! assert_eq!(inputs.value(101).unwrap(), false); // Address 101 (offset 1) -> next bit is 0
//! assert_eq!(inputs.value(102).unwrap(), true);  // Address 102 (offset 2) -> next bit is 1
//! assert_eq!(inputs.value(107).unwrap(), false); // Address 107 (offset 7) -> MSB of 0x05 is 0
//!
//! // Accessing values out of bounds will return an error
//! assert_eq!(inputs.value(99), Err(MbusError::InvalidAddress));
//! assert_eq!(inputs.value(108), Err(MbusError::InvalidAddress));
//!
//! // Get the raw bit-packed bytes (only the first byte is active for 8 inputs)
//! assert_eq!(inputs.values(), &[0x05]);
//! ```

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;

    /// Tests the creation of a new `DiscreteInputs` instance and verifies its properties.
    /// Ensures that the getters correctly return the initialized values.
    #[test]
    fn test_discrete_inputs_new_and_getters() {
        // Prepare bit-packed data: bits 0, 1, and 3 are ON (0b0000_1011 = 0x0B)
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        values[0] = 0b0000_1011;

        // Initialize DiscreteInputs with address 100 and quantity 4
        // Then load the values into the internal fixed-size buffer
        let inputs = DiscreteInputs::new(100, 4)
            .unwrap()
            .with_values(&values, 4)
            .expect("Should successfully load values");

        assert_eq!(inputs.from_address(), 100);
        assert_eq!(inputs.quantity(), 4);
        assert_eq!(inputs.values(), &values[..1]); // 4 bits = 1 byte
    }

    /// Tests retrieving individual discrete input values using the `value` method.
    /// Verifies that bitwise extraction correctly identifies ON (true) and OFF (false) states.
    #[test]
    fn test_discrete_inputs_get_value() {
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        // 0x05 = 0b0000_0101 (Inputs at offsets 0 and 2 are ON)
        values[0] = 0x05;
        // 0x80 = 0b1000_0000 (Input at offset 15 is ON)
        values[1] = 0x80;

        let inputs = DiscreteInputs::new(10, 16)
            .unwrap()
            .with_values(&values, 16)
            .unwrap();

        // Check first byte (address 10-17)
        assert_eq!(inputs.value(10).unwrap(), true); // Offset 0 -> bit 0 is 1
        assert_eq!(inputs.value(11).unwrap(), false); // Offset 1 -> bit 1 is 0
        assert_eq!(inputs.value(12).unwrap(), true); // Offset 2 -> bit 2 is 1
        assert_eq!(inputs.value(17).unwrap(), false); // Offset 7 -> bit 7 is 0

        // Check second byte (address 18-25)
        assert_eq!(inputs.value(18).unwrap(), false); // Offset 8 -> bit 0 of 2nd byte is 0
        assert_eq!(inputs.value(25).unwrap(), true); // Offset 15 -> bit 7 of 2nd byte is 1
    }

    /// Tests that retrieving a value out of the defined range returns an `InvalidAddress` error.
    /// Ensures boundary checks are enforced for read operations.
    #[test]
    fn test_discrete_inputs_get_value_out_of_bounds() {
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        values[0] = 0xFF;

        let inputs = DiscreteInputs::new(10, 8)
            .unwrap()
            .with_values(&values, 8)
            .unwrap();

        // Address below the range [10-17]
        assert_eq!(inputs.value(9), Err(MbusError::InvalidAddress));
        // Address exactly at the upper bound (10 + 8 = 18) is out of bounds
        assert_eq!(inputs.value(18), Err(MbusError::InvalidAddress));
    }
}
