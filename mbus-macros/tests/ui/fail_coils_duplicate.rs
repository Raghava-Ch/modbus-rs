extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidQuantity,
        InvalidByteCount,
        BufferTooSmall,
        InvalidValue,
    }
}

pub trait CoilMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;

    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
    fn write_single(&mut self, address: u16, value: bool) -> Result<(), errors::MbusError>;
    fn write_many_from_packed(
        &mut self,
        address: u16,
        quantity: u16,
        values: &[u8],
        packed_bit_offset: usize,
    ) -> Result<(), errors::MbusError>;
    fn is_batch_notified(_addr: u16) -> bool {
        false
    }
}

use mbus_macros::CoilsModel;

#[derive(Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    a: bool,
    #[coil(addr = 0)]
    b: bool,
}

fn main() {}
