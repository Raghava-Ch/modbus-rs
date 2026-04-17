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

    fn encode(&self, _address: u16, _quantity: u16, _out: &mut [u8]) -> Result<u8, errors::MbusError>;
    fn write_single(&mut self, _address: u16, _value: u16) -> Result<(), errors::MbusError>;
    fn write_many(&mut self, _address: u16, _values: &[u16]) -> Result<(), errors::MbusError>;
    fn is_batch_notified(_addr: u16) -> bool {
        false
    }
}

pub trait InputRegisterMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const WORD_COUNT: usize;

    fn encode(&self, _address: u16, _quantity: u16, _out: &mut [u8]) -> Result<u8, errors::MbusError>;
}

pub trait CoilMap {
    const ADDR_MIN: u16;
    const ADDR_MAX: u16;
    const BIT_COUNT: usize;
    const HAS_BATCH_NOTIFIED_FIELDS: bool = false;

    fn encode(&self, _address: u16, _quantity: u16, _out: &mut [u8]) -> Result<u8, errors::MbusError>;
    fn write_single(&mut self, _address: u16, _value: bool) -> Result<(), errors::MbusError>;
    fn write_many_from_packed(
        &mut self,
        _address: u16,
        _quantity: u16,
        _values: &[u8],
        _packed_bit_offset: usize,
    ) -> Result<(), errors::MbusError>;
    fn is_batch_notified(_addr: u16) -> bool {
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

use mbus_macros::{HoldingRegistersModel, modbus_app};

#[derive(Default, HoldingRegistersModel)]
struct Holding {
    #[reg(addr = 0, notify_via_batch = true)]
    setpoint: u16,
}

#[modbus_app(holding_registers(holding))]
struct App {
    holding: Holding,
}

fn main() {
    let _ = App {
        holding: Holding::default(),
    };
}
