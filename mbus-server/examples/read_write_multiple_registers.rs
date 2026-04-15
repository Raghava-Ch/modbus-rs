//! Minimal `Read/Write Multiple Registers` (FC 0x17) server-side example.
//!
//! Demonstrates implementing [`ModbusAppHandler::read_write_multiple_registers_request`]
//! to service FC 0x17 requests.  Per the Modbus spec the write executes **before**
//! the read; a compliant implementation must observe this order.
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example read_write_multiple_registers
//! ```

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;

/// A minimal in-memory register map backed by a fixed-size array.
struct RegMap {
    regs: [u16; 128],
}

impl RegMap {
    fn new() -> Self {
        Self { regs: [0u16; 128] }
    }
}

impl ModbusAppHandler for RegMap {
    /// FC 0x17 — write the supplied registers, then read back the requested window.
    ///
    /// Per Modbus spec the write happens **first**; the read reflects the newly
    /// written values if the windows overlap.
    fn read_write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        // --- Write phase (first) ---
        for (i, &v) in write_values.iter().enumerate() {
            let addr = write_address as usize + i;
            if addr >= self.regs.len() {
                return Err(MbusError::InvalidAddress);
            }
            self.regs[addr] = v;
        }

        // --- Read phase (second) ---
        for i in 0..read_quantity as usize {
            let addr = read_address as usize + i;
            if addr >= self.regs.len() {
                return Err(MbusError::InvalidAddress);
            }
            let value = self.regs[addr];
            out[i * 2] = (value >> 8) as u8;
            out[i * 2 + 1] = value as u8;
        }

        Ok((read_quantity * 2) as u8)
    }
}

fn main() -> Result<(), MbusError> {
    let mut map = RegMap::new();

    // Simulate an FC 0x17 call:
    //   write [0xABCD, 0x1234] starting at address 10
    //   read  3 registers       starting at address 9
    //
    // After write:  reg[10] = 0xABCD, reg[11] = 0x1234
    // Read window:  reg[9]  = 0x0000 (unchanged),
    //               reg[10] = 0xABCD (just written),
    //               reg[11] = 0x1234 (just written)

    let write_values = [0xABCDu16, 0x1234];
    let mut out = [0u8; 6]; // 3 registers × 2 bytes

    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");
    let n = map.read_write_multiple_registers_request(
        1, // txn_id
        uid,
        9,  // read_address
        3,  // read_quantity
        10, // write_address
        &write_values,
        &mut out,
    )?;

    assert_eq!(n, 6);
    println!(
        "read {} bytes: {:04X} {:04X} {:04X}",
        n,
        u16::from_be_bytes([out[0], out[1]]),
        u16::from_be_bytes([out[2], out[3]]),
        u16::from_be_bytes([out[4], out[5]]),
    );
    // Expected output: 0000 ABCD 1234

    Ok(())
}
