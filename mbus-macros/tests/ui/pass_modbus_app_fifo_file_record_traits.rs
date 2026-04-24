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

    pub trait ServerFifoHandler {}
    pub trait ServerFileRecordHandler {}
    pub trait ServerDiagnosticsHandler {}
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

use mbus_macros::modbus_app;

struct LowRange;
impl HoldingRegisterMap for LowRange {
    const ADDR_MIN: u16 = 0;
    const ADDR_MAX: u16 = 5;
    const WORD_COUNT: usize = 6;

    fn encode(&self, _address: u16, _quantity: u16, _out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(0)
    }
    fn write_single(&mut self, _address: u16, _value: u16) -> Result<(), errors::MbusError> {
        Ok(())
    }
    fn write_many(&mut self, _address: u16, _values: &[u16]) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

struct HighRange;
impl HoldingRegisterMap for HighRange {
    const ADDR_MIN: u16 = 10;
    const ADDR_MAX: u16 = 15;
    const WORD_COUNT: usize = 6;

    fn encode(&self, _address: u16, _quantity: u16, _out: &mut [u8]) -> Result<u8, errors::MbusError> {
        Ok(0)
    }
    fn write_single(&mut self, _address: u16, _value: u16) -> Result<(), errors::MbusError> {
        Ok(())
    }
    fn write_many(&mut self, _address: u16, _values: &[u16]) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

#[modbus_app(holding_registers(low, high))]
struct App {
    low: LowRange,
    high: HighRange,
}

fn assert_fifo_file_record_impls<T: app::ServerFifoHandler + app::ServerFileRecordHandler>() {}

fn main() {
    assert_fifo_file_record_impls::<App>();
}
