extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidValue,
        BufferTooSmall,
    }
}

pub trait InputRegisterMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
}

use mbus_macros::InputRegistersModel;

#[derive(Default, InputRegistersModel)]
struct InputRegs {
    #[reg(addr = 0)]
    volts: u16,
    #[reg(addr = 1, scale = 0.1, unit = "A")]
    amps: u16,
}

fn main() {
    let mut regs = InputRegs::default();
    regs.set_volts(230);
    regs.set_amps(120);

    let _ = regs.volts();
    let _ = regs.amps_scaled();
    let _ = InputRegs::amps_unit();

    let mut out = [0u8; 4];
    let _ = InputRegisterMap::encode(&regs, 0, 2, &mut out).unwrap();
}
