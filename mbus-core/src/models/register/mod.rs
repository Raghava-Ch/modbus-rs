//! # Modbus Register Models
//!
//! This module provides the data structures and logic for handling **Holding Registers**
//! (Function Codes 0x03, 0x06, 0x10) and **Input Registers** (Function Code 0x04).
//!
//! Registers in Modbus are 16-bit unsigned integers (u16). They are the primary
//! data type for analog values, configuration parameters, and multi-word data types
//! like floating-point numbers or 32-bit integers.
//!
//! ## Key Features
//! - **Generic Capacity**: Uses Rust generics to allow fixed-size buffers tailored to
//!   specific memory constraints, defaulting to the protocol maximum of 125 registers.
//! - **no_std Compatible**: Designed for embedded systems without heap allocation.
//! - **Address Mapping**: Provides safe methods to access registers by their absolute
//!   Modbus address rather than just array indices.
//! - **Boundary Protection**: Ensures all read and write operations stay within the
//!   defined address range and PDU size limits.
mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;

    #[test]
    fn test_registers_new_success() {
        let from_addr = 100;
        let quantity = 10;
        let regs = Registers::<20>::new(from_addr, quantity).unwrap();

        assert_eq!(regs.from_address(), from_addr);
        assert_eq!(regs.quantity(), quantity);
        // Ensure values are initialized to zero
        assert_eq!(regs.values()[0], 0);
    }

    #[test]
    fn test_registers_new_invalid_quantity() {
        // Attempt to create registers with quantity exceeding static capacity N
        let res = Registers::<5>::new(100, 6);
        assert!(matches!(res, Err(MbusError::InvalidQuantity)));
    }

    #[test]
    fn test_registers_with_values_success() {
        let regs = Registers::<10>::new(100, 5).unwrap();
        let data = [1, 2, 3, 4, 5];

        let regs = regs.with_values(&data, 5).unwrap();

        assert_eq!(regs.values()[0], 1);
        assert_eq!(regs.values()[4], 5);
        assert_eq!(regs.values()[5], 0); // Beyond length should remain 0
    }

    #[test]
    fn test_registers_with_values_bounds_error() {
        let regs = Registers::<10>::new(100, 5).unwrap();
        let data = [1, 2, 3, 4, 5, 6];

        // Error: length (6) > quantity (5)
        let res = regs.with_values(&data, 6);
        assert!(matches!(res, Err(MbusError::InvalidQuantity)));
    }

    #[test]
    fn test_set_and_get_value_success() {
        let mut regs = Registers::<10>::new(100, 5).unwrap();

        // Set value at address 102 (index 2)
        regs.set_value(102, 0xABCD).unwrap();

        assert_eq!(regs.value(102).unwrap(), 0xABCD);
        assert_eq!(regs.values()[2], 0xABCD);
    }

    #[test]
    fn test_set_value_out_of_range() {
        let mut regs = Registers::<10>::new(100, 5).unwrap();

        // Lower bound check
        assert!(matches!(
            regs.set_value(99, 1),
            Err(MbusError::InvalidAddress)
        ));
        // Upper bound check (100 + 5 = 105 is exclusive)
        assert!(matches!(
            regs.set_value(105, 1),
            Err(MbusError::InvalidAddress)
        ));
    }

    #[test]
    fn test_get_value_out_of_range() {
        let regs = Registers::<10>::new(100, 5).unwrap();
        assert!(matches!(regs.value(105), Err(MbusError::InvalidAddress)));
    }

    #[test]
    fn test_default_capacity() {
        // Verify that the default generic N is MAX_REGISTERS_PER_PDU (125)
        let regs: Registers = Registers::new(0, 125).unwrap();
        assert_eq!(regs.values().len(), 125);
    }
}
