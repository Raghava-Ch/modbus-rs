//! # Modbus Coil Models
//!
//! This module provides the data structures used to represent Modbus Coils.
//! In the Modbus protocol, coils are single-bit boolean values representing discrete
//! outputs that can be both read and written by the client.
//!
//! The primary component of this module is the [`Coils`] struct, which safely abstracts
//! the complex bit-packing and unpacking operations defined by the protocol into a clean API.
//! # Examples
//!
//! ```rust
//! use mbus_core::models::coil::{Coils, CoilState};
//! use mbus_core::errors::MbusError;
//!
//! // Initialize a block of 8 coils starting at Modbus address 100.
//! // Initially all coils are OFF (0).
//! let mut coils = Coils::new(100, 8).unwrap();
//!
//! // Verify initial state: all coils are OFF
//! assert_eq!(coils.value(100).unwrap(), CoilState::Off);
//! assert_eq!(coils.value(107).unwrap(), CoilState::Off);
//!
//! // Set coil at address 100 (offset 0) to ON
//! coils.set_value(100, CoilState::On).unwrap();
//! assert_eq!(coils.value(100).unwrap(), CoilState::On);
//! assert_eq!(coils.values()[..1], [0b0000_0001]);
//!
//! // Set coil at address 102 (offset 2) to ON
//! coils.set_value(102, CoilState::On).unwrap();
//! assert_eq!(coils.value(102).unwrap(), CoilState::On);
//! assert_eq!(coils.raw_values()[..1], [0b0000_0101]);
//!
//! // Set coil at address 100 back to OFF
//! coils.set_value(100, CoilState::Off).unwrap();
//! assert_eq!(coils.value(100).unwrap(), CoilState::Off);
//! assert_eq!(coils.raw_values()[..1], [0b0000_0100]);
//!
//! // Example with `with_raw_values` for loading pre-packed data
//! let pre_packed_data = [0b1010_1010, 0b0101_0101]; // Two bytes for 16 coils
//! let mut loaded_coils = Coils::new(200, 16).unwrap()
//!     .with_raw_values(&pre_packed_data, 16)
//!     .expect("Valid quantity and data");
//!
//! assert_eq!(loaded_coils.value(200).unwrap(), CoilState::Off); // LSB of 0b1010_1010 is 0
//! assert_eq!(loaded_coils.value(201).unwrap(), CoilState::On);  // Next bit is 1
//! assert_eq!(loaded_coils.value(208).unwrap(), CoilState::On);  // LSB of 0b0101_0101 is 1 (first bit of second byte)
//! ```

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;

    /// Test the creation of a new Coils instance and verify its properties.
    /// This test ensures that the `Coils` struct correctly stores the starting address,
    /// quantity, and bit-packed values without using dynamic allocation or heapless vectors.
    #[test]
    fn test_coils_new() {
        // Initialize a fixed-size array for coil states (250 bytes for up to 2000 coils)
        let mut values = [0u8; MAX_COIL_BYTES];
        values[0] = 0b0000_1011; // Coils at offsets 0, 1, and 3 are set to ON (1)

        // Initialize Coils with address 100 and quantity 8
        // Then load the values into the internal fixed-size buffer using the builder pattern
        let coils = Coils::new(100, 8)
            .unwrap()
            .with_raw_values(&values, 8)
            .expect("Should successfully load values");

        assert_eq!(coils.from_address(), 100);
        assert_eq!(coils.quantity(), 8);
        // Verify that the internal state matches the input array
        assert_eq!(coils.raw_values(), &values);
    }

    /// Test retrieving individual coil values using the `value` method.
    /// Verifies that bitwise extraction correctly identifies ON/OFF states.
    #[test]
    fn test_coils_get_value() {
        let mut values = [0u8; MAX_COIL_BYTES];
        // 0x05 = 0b0000_0101 (Coils at offsets 0 and 2 are ON)
        values[0] = 0x05;

        let coils = Coils::new(10, 8)
            .unwrap()
            .with_raw_values(&values, 8)
            .unwrap();

        // Check specific bits based on the 0x05 bitmask
        assert_eq!(coils.value(10).unwrap(), CoilState::On); // Address 10 (Offset 0) -> bit 0 is 1
        assert_eq!(coils.value(11).unwrap(), CoilState::Off); // Address 11 (Offset 1) -> bit 1 is 0
        assert_eq!(coils.value(12).unwrap(), CoilState::On); // Address 12 (Offset 2) -> bit 2 is 1
        assert_eq!(coils.value(17).unwrap(), CoilState::Off); // Address 17 (Offset 7) -> bit 7 is 0
    }

    /// Test that retrieving a value out of the defined range returns an error.
    /// Ensures boundary checks are enforced for read operations.
    #[test]
    fn test_coils_get_value_out_of_bounds() {
        let values = [0u8; MAX_COIL_BYTES];
        let coils = Coils::new(10, 8)
            .unwrap()
            .with_raw_values(&values, 8)
            .unwrap();

        // Address below the range [10-17]
        assert_eq!(coils.value(9), Err(MbusError::InvalidAddress));
        // Address exactly at the upper bound (10 + 8 = 18) is out of bounds
        assert_eq!(coils.value(18), Err(MbusError::InvalidAddress));
    }

    /// Test setting individual coil values and verify the bitwise modifications.
    /// This confirms that `set_value` correctly updates specific bits within the byte array.
    #[test]
    fn test_coils_set_value() {
        let values = [0u8; MAX_COIL_BYTES];

        // Manage 16 coils starting at address 20 (spans 2 bytes)
        let mut coils = Coils::new(20, 16)
            .unwrap()
            .with_raw_values(&values, 16)
            .unwrap();

        // Set coil at target address 22 (base 20 + offset 2) to ON
        assert_eq!(coils.set_value(22, CoilState::On), Ok(()));
        assert_eq!(coils.value(22).unwrap(), CoilState::On);

        // Set coil at target address 30 (base 25 + offset 5) to ON
        // This tests address calculation logic: 25 + 5 = 30
        assert_eq!(coils.set_value(30, CoilState::On), Ok(()));
        assert_eq!(coils.value(30).unwrap(), CoilState::On);

        // Turn coil at target address 22 back to OFF
        assert_eq!(coils.set_value(22, CoilState::Off), Ok(()));
        assert_eq!(coils.value(22).unwrap(), CoilState::Off);

        // Verify that modifying one bit did not affect others (coil 30 should remain ON)
        assert_eq!(coils.value(30).unwrap(), CoilState::On);
    }

    /// Test that setting a value out of the defined range returns an error.
    /// Ensures boundary checks are enforced for write operations to prevent memory corruption.
    #[test]
    fn test_coils_set_value_out_of_bounds() {
        let values = [0u8; MAX_COIL_BYTES];
        let mut coils = Coils::new(10, 8)
            .unwrap()
            .with_raw_values(&values, 8)
            .unwrap();

        // Trying to set address 18 (10 base + 8 offset = 18), which is out of range [10, 17]
        assert_eq!(coils.set_value(18, CoilState::On), Err(MbusError::InvalidAddress));
        // Target address totally outside the managed block range
        assert_eq!(coils.set_value(50, CoilState::On), Err(MbusError::InvalidAddress));
    }
}
