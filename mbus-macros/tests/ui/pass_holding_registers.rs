extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidValue,
        InvalidQuantity,
        BufferTooSmall,
    }
}

pub trait HoldingRegisterMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
    fn write_single(&mut self, address: u16, value: u16) -> Result<(), errors::MbusError>;
    fn write_many(&mut self, address: u16, values: &[u16]) -> Result<(), errors::MbusError>;
}

use mbus_macros::HoldingRegistersModel;

#[derive(Default, HoldingRegistersModel)]
struct Holding {
    #[reg(addr = 0)]
    volts: u16,
    #[reg(addr = 1, scale = 0.1, unit = "A")]
    amps: u16,
}

fn main() {
    let mut h = Holding::default();
    h.set_volts(230);
    h.set_amps(120);
    let _ = h.amps_scaled();
    let _ = Holding::amps_unit();

    let mut out = [0u8; 4];
    let _ = HoldingRegisterMap::encode(&h, 0, 2, &mut out).unwrap();
    HoldingRegisterMap::write_single(&mut h, 0, 231).unwrap();
    HoldingRegisterMap::write_many(&mut h, 0, &[220, 100]).unwrap();
}
