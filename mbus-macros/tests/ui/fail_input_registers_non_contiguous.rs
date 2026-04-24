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
struct NonContiguousInputRegs {
    #[reg(addr = 0)]
    first: u16,
    #[reg(addr = 2)]
    third: u16,
}

fn main() {
    let _ = NonContiguousInputRegs::default();
}
