use heapless::Vec;

use crate::errors::MbusError;

/// Maximum number of registers that can be read/written in a single Modbus PDU (125 registers).
pub const MAX_REGISTERS_PER_PDU: usize = 125;

/// Represents the state of a block of registers read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Registers {
    /// The starting address of the first register in this block.
    from_address: u16,
    /// The number of registers in this block.
    quantity: u16,
    /// The register values.
    values: Vec<u16, MAX_REGISTERS_PER_PDU>,
}

impl Registers {
    /// Creates a new `Registers` instance.
    pub fn new(from_address: u16, quantity: u16, values: Vec<u16, MAX_REGISTERS_PER_PDU>) -> Self {
        Self {
            from_address,
            quantity,
            values,
        }
    }

    /// Returns the starting address.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the quantity of registers.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns the register values.
    pub fn values(&self) -> &Vec<u16, MAX_REGISTERS_PER_PDU> {
        &self.values
    }

    /// Retrieves the value of a specific register by its address.
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

#[cfg(test)]
mod tests {
    use super::*;
    // --- Registers struct tests ---

    /// Test case: `Registers::value` returns the correct value for a valid address.
    #[test]
    fn test_registers_value_valid() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        values_vec.push(0x5678).unwrap();
        let registers = Registers::new(0x0000, 2, values_vec);

        assert_eq!(registers.value(0x0000).unwrap(), 0x1234);
        assert_eq!(registers.value(0x0001).unwrap(), 0x5678);
    }

    /// Test case: `Registers::value` returns an error for an address below the range.
    #[test]
    fn test_registers_value_invalid_address_low() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        let registers = Registers::new(0x0001, 1, values_vec);

        assert_eq!(
            registers.value(0x0000).unwrap_err(),
            MbusError::InvalidAddress
        );
    }

    /// Test case: `Registers::value` returns an error for an address above the range.
    #[test]
    fn test_registers_value_invalid_address_high() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        let registers = Registers::new(0x0000, 1, values_vec);

        assert_eq!(
            registers.value(0x0001).unwrap_err(),
            MbusError::InvalidAddress
        );
    }
}
