//! # Modbus Register Models
//!
//! This module defines the data structures for handling **Holding Registers** (FC 0x03, 0x06, 0x10)
//! and **Input Registers** (FC 0x04).
//!
//! In Modbus, registers are 16-bit unsigned integers.
//! - **Holding Registers**: Read-write registers used for configuration, setpoints, and control.
//! - **Input Registers**: Read-only registers typically used for sensor data or status.
//!
//! ## Key Components
//! - [`Registers`]: A container for a block of 16-bit register values.
//! - [`MAX_REGISTERS_PER_PDU`]: The protocol limit for registers in a single request/response.
//!
//! ## Protocol Limits
//! According to the Modbus specification, a single PDU can carry up to 125 registers (250 bytes).
//! This implementation uses a generic constant `N` to allow for smaller, memory-optimized
//! allocations in `no_std` environments while defaulting to the protocol maximum.

use crate::errors::MbusError;

/// Maximum number of registers that can be read/written in a single Modbus PDU (125 registers).
pub const MAX_REGISTERS_PER_PDU: usize = 125;

/// Represents the state of a block of registers read from a Modbus server.
///
/// This structure maintains the starting address and the quantity of registers,
/// providing safe accessors to individual values within the block.
///
/// # Type Parameters
/// * `N` - The internal storage capacity, defaults to [`MAX_REGISTERS_PER_PDU`].
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Registers<const N: usize = MAX_REGISTERS_PER_PDU> {
    /// The starting address of the first register in this block.
    from_address: u16,
    /// The number of registers in this block.
    quantity: u16,
    /// The register values.
    values: [u16; N],
}

impl<const N: usize> Registers<N> {
    /// Creates a new `Registers` instance.
    ///
    /// # Arguments
    /// * `from_address` - The starting Modbus address.
    /// * `quantity` - The number of registers to be managed in this block.
    ///
    pub fn new(from_address: u16, quantity: u16) -> Result<Self, MbusError> {
        if quantity as usize > N {
            return Err(MbusError::InvalidQuantity);
        }
        Ok(Self {
            from_address,
            quantity,
            values: [0; N],
        })
    }

    /// Loads register values into the model and validates the length against capacity.
    ///
    /// # Arguments
    /// * `values` - A slice of 16-bit values to copy into the internal buffer.
    /// * `length` - The number of registers being loaded.
    pub fn with_values(mut self, values: &[u16], length: u16) -> Result<Self, MbusError> {
        if length > N as u16 {
            return Err(MbusError::InvalidQuantity);
        }
        if length > self.quantity {
            return Err(MbusError::InvalidQuantity);
        }
        self.values[..length as usize].copy_from_slice(values);

        Ok(self)
    }

    /// Returns the starting Modbus address of the first register.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the number of registers currently held in this block.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns the register values.
    pub fn values(&self) -> &[u16; N] {
        &self.values
    }

    /// Updates the value of a specific register within the block.
    ///
    /// # Arguments
    /// * `address` - The Modbus address of the register to update.
    /// * `value` - The new 16-bit unsigned integer value.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidAddress` if the address is outside the range
    /// defined by `from_address` and `quantity`.
    pub fn set_value(&mut self, address: u16, value: u16) -> Result<(), MbusError> {
        // Check if the address is within the bounds of this register block
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }

        // Calculate the local index and update the value
        let index = (address - self.from_address) as usize;
        self.values[index] = value;

        Ok(())
    }

    /// Retrieves the value of a specific register by its address.
    ///
    /// # Arguments
    /// * `address` - The Modbus address to query.
    ///
    /// # Errors
    /// Returns `MbusError::InvalidAddress` if the address is outside the block range.
    pub fn value(&self, address: u16) -> Result<u16, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let index = (address - self.from_address) as usize;
        self.values
            .get(index)
            .copied()
            .ok_or(MbusError::InvalidAddress)
    }
}
