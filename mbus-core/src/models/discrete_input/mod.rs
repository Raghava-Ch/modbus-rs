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
//! and wraps [`Coils`](crate::models::coil::Coils) as a read-only container for discrete inputs.
//!
//! # Examples
//!
//! ```rust
//! use mbus_core::models::discrete_input::{DiscreteInputs, DiscreteInputState, MAX_DISCRETE_INPUT_BYTES};
//! use mbus_core::errors::MbusError;
//!
//! // Initialize a block of 8 discrete inputs starting at Modbus address 100.
//! // Initially all inputs are OFF.
//! let mut inputs = DiscreteInputs::new(100, 8).unwrap();
//!
//! // Verify initial state: all inputs are OFF
//! assert_eq!(inputs.value(100).unwrap(), DiscreteInputState::Off);
//! assert_eq!(inputs.value(107).unwrap(), DiscreteInputState::Off);
//!
//! // Simulate receiving data where inputs at offsets 0 and 2 are ON (0b0000_0101)
//! let received_data = [0x05, 0x00, 0x00, 0x00];
//! inputs = inputs.with_values(&received_data, 8).expect("Valid quantity and data");
//!
//! // Read individual input values
//! assert_eq!(inputs.value(100).unwrap(), DiscreteInputState::On);
//! assert_eq!(inputs.value(101).unwrap(), DiscreteInputState::Off);
//! assert_eq!(inputs.value(102).unwrap(), DiscreteInputState::On);
//! assert_eq!(inputs.value(107).unwrap(), DiscreteInputState::Off);
//!
//! // Accessing values out of bounds will return an error
//! assert_eq!(inputs.value(99), Err(MbusError::InvalidAddress));
//! assert_eq!(inputs.value(108), Err(MbusError::InvalidAddress));
//!
//! // Get the raw bit-packed bytes
//! assert_eq!(inputs.values(), &[0x05]);
//! ```

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;

    /// Tests the creation of a new `DiscreteInputs` instance and verifies its properties.
    #[test]
    fn test_discrete_inputs_new_and_getters() {
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        values[0] = 0b0000_1011;

        let inputs = DiscreteInputs::new(100, 4)
            .unwrap()
            .with_values(&values, 4)
            .expect("Should successfully load values");

        assert_eq!(inputs.from_address(), 100);
        assert_eq!(inputs.quantity(), 4);
        assert_eq!(inputs.values(), &values[..1]);
    }

    /// Tests retrieving individual discrete input values using the `value` method.
    #[test]
    fn test_discrete_inputs_get_value() {
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        values[0] = 0x05;
        values[1] = 0x80;

        let inputs = DiscreteInputs::new(10, 16)
            .unwrap()
            .with_values(&values, 16)
            .unwrap();

        // Check first byte (address 10-17)
        assert_eq!(inputs.value(10).unwrap(), DiscreteInputState::On);
        assert_eq!(inputs.value(11).unwrap(), DiscreteInputState::Off);
        assert_eq!(inputs.value(12).unwrap(), DiscreteInputState::On);
        assert_eq!(inputs.value(17).unwrap(), DiscreteInputState::Off);

        // Check second byte (address 18-25)
        assert_eq!(inputs.value(18).unwrap(), DiscreteInputState::Off);
        assert_eq!(inputs.value(25).unwrap(), DiscreteInputState::On);
    }

    /// Tests that retrieving a value out of the defined range returns an `InvalidAddress` error.
    #[test]
    fn test_discrete_inputs_get_value_out_of_bounds() {
        let mut values = [0u8; MAX_DISCRETE_INPUT_BYTES];
        values[0] = 0xFF;

        let inputs = DiscreteInputs::new(10, 8)
            .unwrap()
            .with_values(&values, 8)
            .unwrap();

        assert_eq!(inputs.value(9), Err(MbusError::InvalidAddress));
        assert_eq!(inputs.value(18), Err(MbusError::InvalidAddress));
    }
}
