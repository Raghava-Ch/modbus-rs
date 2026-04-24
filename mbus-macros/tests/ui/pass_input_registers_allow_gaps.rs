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
#[reg(allow_gaps)]
struct SparseInputRegs {
    #[reg(addr = 0)]
    phase_a: u16,
    #[reg(addr = 2)]
    phase_c: u16,
}

fn main() {
    let mut regs = SparseInputRegs::default();
    regs.set_phase_a(111);
    regs.set_phase_c(333);

    let _ = regs.phase_a();
    let _ = regs.phase_c();

    let mut out = [0u8; 2];
    let _ = InputRegisterMap::encode(&regs, 0, 1, &mut out).unwrap();
}
