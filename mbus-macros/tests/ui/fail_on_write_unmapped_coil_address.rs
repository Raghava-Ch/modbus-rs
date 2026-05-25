#![allow(unexpected_cfgs)]

extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidValue,
        InvalidQuantity,
        BufferTooSmall,
        InvalidByteCount,
    }
}

pub trait HoldingRegisterMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;
    const HAS_BATCH_NOTIFIED_FIELDS: bool = false;

    #[allow(unused_variables)]
    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
    #[allow(unused_variables)]
    fn write_single(&mut self, address: u16, value: u16) -> Result<(), errors::MbusError>;
    #[allow(unused_variables)]
    fn write_many(&mut self, address: u16, values: &[u16]) -> Result<(), errors::MbusError>;
    #[allow(unused_variables)]
    fn is_batch_notified(addr: u16) -> bool {
        false
    }
}

pub trait InputRegisterMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;

    #[allow(unused_variables)]
    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
}

pub trait CoilMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;
    const HAS_BATCH_NOTIFIED_FIELDS: bool = false;

    #[allow(unused_variables)]
    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError>;
    #[allow(unused_variables)]
    fn write_single(&mut self, address: u16, value: bool) -> Result<(), errors::MbusError>;
    #[allow(unused_variables)]
    fn write_many_from_packed(
        &mut self,
        address: u16,
        quantity: u16,
        values: &[u8],
        packed_bit_offset: usize,
    ) -> Result<(), errors::MbusError>;
    #[allow(unused_variables)]
    fn is_batch_notified(addr: u16) -> bool {
        false
    }
}

pub mod transport {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UnitIdOrSlaveAddr(u8);

    impl UnitIdOrSlaveAddr {
        pub fn get(self) -> u8 {
            self.0
        }
    }
}

pub mod app {
    pub trait ServerExceptionHandler {}
    pub trait ServerCoilHandler {}
    pub trait ServerDiscreteInputHandler {}
    pub trait ServerHoldingRegisterHandler {}
    pub trait ServerInputRegisterHandler {}
    pub trait ServerFifoHandler {}
    pub trait ServerFileRecordHandler {}
    pub trait ServerDiagnosticsHandler {}
}

use mbus_macros::{CoilsModel, modbus_app};

#[derive(Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    run_enable: bool,
}

#[modbus_app(coils(coils, on_write_99 = on_run_enable))]
struct App {
    coils: Coils,
}

impl App {
    #[allow(unused_variables)]
    fn on_run_enable(&mut self, address: u16, old: bool, new: bool) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

fn main() {
    let _ = App {
        coils: Coils::default(),
    };
}
