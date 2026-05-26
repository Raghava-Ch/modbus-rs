#![allow(unexpected_cfgs)]

extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidQuantity,
        BufferTooSmall,
    }
}

pub mod transport {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct UnitIdOrSlaveAddr(pub u8);
}

pub mod app {
    use crate::errors::MbusError;
    use crate::transport::UnitIdOrSlaveAddr;

    pub trait ServerExceptionHandler {}

    pub trait ServerCoilHandler {}

    pub trait ServerDiscreteInputHandler {}

    pub trait ServerHoldingRegisterHandler {
        #[allow(unused_variables)]
        fn read_multiple_holding_registers_request(
            &mut self,
            txn_id: u16,
            unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            quantity: u16,
            out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }

        #[allow(unused_variables)]
        fn write_single_register_request(
            &mut self,
            txn_id: u16,
            unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            value: u16,
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }

        #[allow(unused_variables)]
        fn write_multiple_registers_request(
            &mut self,
            txn_id: u16,
            unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            starting_address: u16,
            values: &[u16],
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }

    pub trait ServerInputRegisterHandler {
        #[allow(unused_variables)]
        fn read_multiple_input_registers_request(
            &mut self,
            txn_id: u16,
            unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            address: u16,
            quantity: u16,
            out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }

    pub trait ServerFifoHandler {}
    pub trait ServerFileRecordHandler {}
    pub trait ServerDiagnosticsHandler {}
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

use mbus_macros::modbus_app;

struct HighRange;
impl HoldingRegisterMap for HighRange {
    const ADDR_MIN: u16 = 10;
    const ADDR_MAX: u16 = 15;
    const WORD_COUNT: usize = 6;

    #[allow(unused_variables)]
    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(0)
    }

    #[allow(unused_variables)]
    fn write_single(&mut self, address: u16, value: u16) -> Result<(), errors::MbusError> {
        Ok(())
    }

    #[allow(unused_variables)]
    fn write_many(&mut self, address: u16, values: &[u16]) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

struct LowRange;
impl HoldingRegisterMap for LowRange {
    const ADDR_MIN: u16 = 0;
    const ADDR_MAX: u16 = 5;
    const WORD_COUNT: usize = 6;

    #[allow(unused_variables)]
    fn encode(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(0)
    }

    #[allow(unused_variables)]
    fn write_single(&mut self, address: u16, value: u16) -> Result<(), errors::MbusError> {
        Ok(())
    }

    #[allow(unused_variables)]
    fn write_many(&mut self, address: u16, values: &[u16]) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

#[modbus_app(holding_registers(high, low))]
struct App {
    high: HighRange,
    low: LowRange,
}

fn main() {}
