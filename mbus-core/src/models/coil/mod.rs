//! # Modbus Coil Models
//!
//! This module provides the data structures used to represent Modbus Coils.
//! In the Modbus protocol, coils are single-bit boolean values representing discrete
//! outputs that can be both read and written by the client.
//!
//! The primary component of this module is the [`Coils`] struct, which safely abstracts
//! the complex bit-packing and unpacking operations defined by the protocol into a clean API.

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
        
        // Create a block of 8 coils starting at Modbus address 100
        let mut coils = Coils::new(100, 8);
        // Load the raw byte values into the model
        coils.set_values(&values, 8).unwrap();
        
        assert_eq!(coils.from_address(), 100);
        assert_eq!(coils.quantity(), 8);
        // Verify that the internal state matches the input array
        assert_eq!(coils.values(), &values);
    }

    /// Test retrieving individual coil values using the `value` method.
    /// Verifies that bitwise extraction correctly identifies ON/OFF states.
    #[test]
    fn test_coils_get_value() {
        let mut values = [0u8; MAX_COIL_BYTES];
        // 0x05 = 0b0000_0101 (Coils at offsets 0 and 2 are ON)
        values[0] = 0x05;
        
        let mut coils = Coils::new(10, 8);
        coils.set_values(&values, 8).unwrap();
        
        // Check specific bits based on the 0x05 bitmask
        assert_eq!(coils.value(10).unwrap(), true);  // Address 10 (Offset 0) -> bit 0 is 1
        assert_eq!(coils.value(11).unwrap(), false); // Address 11 (Offset 1) -> bit 1 is 0
        assert_eq!(coils.value(12).unwrap(), true);  // Address 12 (Offset 2) -> bit 2 is 1
        assert_eq!(coils.value(17).unwrap(), false); // Address 17 (Offset 7) -> bit 7 is 0
    }

    /// Test that retrieving a value out of the defined range returns an error.
    /// Ensures boundary checks are enforced for read operations.
    #[test]
    fn test_coils_get_value_out_of_bounds() {
        let values = [0u8; MAX_COIL_BYTES];
        let mut coils = Coils::new(10, 8);
        coils.set_values(&values, 8).unwrap();
        
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
        let mut coils = Coils::new(20, 16);
        coils.set_values(&values, 16).unwrap();

        // Set coil at target address 22 (base 20 + offset 2) to ON
        assert_eq!(coils.set_value(22, true), Ok(()));
        assert_eq!(coils.value(22).unwrap(), true);

        // Set coil at target address 30 (base 25 + offset 5) to ON
        // This tests address calculation logic: 25 + 5 = 30
        assert_eq!(coils.set_value(30, true), Ok(()));
        assert_eq!(coils.value(30).unwrap(), true);

        // Turn coil at target address 22 back to OFF
        assert_eq!(coils.set_value(22, false), Ok(()));
        assert_eq!(coils.value(22).unwrap(), false);
        
        // Verify that modifying one bit did not affect others (coil 30 should remain ON)
        assert_eq!(coils.value(30).unwrap(), true);
    }

    /// Test that setting a value out of the defined range returns an error.
    /// Ensures boundary checks are enforced for write operations to prevent memory corruption.
    #[test]
    fn test_coils_set_value_out_of_bounds() {
        let values = [0u8; MAX_COIL_BYTES];
        let mut coils = Coils::new(10, 8);
        coils.set_values(&values, 8).unwrap();

        // Trying to set address 18 (10 base + 8 offset = 18), which is out of range [10, 17]
        assert_eq!(coils.set_value(18, true), Err(MbusError::InvalidAddress));
        // Target address totally outside the managed block range
        assert_eq!(coils.set_value(50, true), Err(MbusError::InvalidAddress));
    }
}
