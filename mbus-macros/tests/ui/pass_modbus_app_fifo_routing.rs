#![allow(unexpected_cfgs)]

extern crate self as mbus_core;
extern crate self as mbus_server;

pub mod errors {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MbusError {
        InvalidAddress,
        InvalidFunctionCode,
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
    pub trait ServerCoilHandler {
        fn read_coils_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _quantity: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
        fn write_single_coil_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _value: bool,
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }
        fn write_multiple_coils_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _starting_address: u16,
            _quantity: u16,
            _values: &[u8],
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }
    pub trait ServerDiscreteInputHandler {
        fn read_discrete_inputs_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _quantity: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }
    pub trait ServerHoldingRegisterHandler {
        fn read_multiple_holding_registers_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _quantity: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
        fn write_single_register_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _value: u16,
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }
        fn write_multiple_registers_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _starting_address: u16,
            _values: &[u16],
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }
    pub trait ServerInputRegisterHandler {
        fn read_multiple_input_registers_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _address: u16,
            _quantity: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }
    pub trait ServerFifoHandler {
        fn read_fifo_queue_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _pointer_address: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidAddress)
        }
    }
    pub trait ServerFileRecordHandler {}
    pub trait ServerDiagnosticsHandler {}
}

/// `FifoQueue` map trait (mirrors mbus_server::FifoQueue).
pub trait FifoQueue {
    const POINTER_ADDRESS: u16;
    fn read_fifo_queue(&mut self, out: &mut [u8]) -> Result<u8, errors::MbusError>;
}

use mbus_macros::modbus_app;

struct Temps;
impl FifoQueue for Temps {
    const POINTER_ADDRESS: u16 = 0x0100;
    fn read_fifo_queue(&mut self, _out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(2)
    }
}

struct Pressures;
impl FifoQueue for Pressures {
    const POINTER_ADDRESS: u16 = 0x0200;
    fn read_fifo_queue(&mut self, _out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(2)
    }
}

#[modbus_app(fifo(temps, pressures))]
struct App {
    temps: Temps,
    pressures: Pressures,
}

fn assert_fifo_handler<T: app::ServerFifoHandler>() {}

fn main() {
    assert_fifo_handler::<App>();
}
