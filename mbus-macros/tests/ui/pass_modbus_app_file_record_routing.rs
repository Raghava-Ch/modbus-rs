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
    pub trait ServerFifoHandler {}
    pub trait ServerFileRecordHandler {
        fn read_file_record_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _file_number: u16,
            _record_number: u16,
            _record_length: u16,
            _out: &mut [u8],
        ) -> Result<u8, MbusError> {
            Err(MbusError::InvalidFunctionCode)
        }
        fn write_file_record_request(
            &mut self,
            _txn_id: u16,
            _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
            _file_number: u16,
            _record_number: u16,
            _record_length: u16,
            _record_data: &[u16],
        ) -> Result<(), MbusError> {
            Err(MbusError::InvalidFunctionCode)
        }
    }
    pub trait ServerDiagnosticsHandler {}
}

/// `FileRecord` map trait (mirrors mbus_server::FileRecord).
pub trait FileRecord {
    const FILE_NUMBER: u16;
    fn read_record(
        &mut self,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, errors::MbusError>;
    fn write_record(
        &mut self,
        record_number: u16,
        record_length: u16,
        data: &[u16],
    ) -> Result<(), errors::MbusError>;
}

use mbus_macros::modbus_app;

struct CalibrationFile;
impl FileRecord for CalibrationFile {
    const FILE_NUMBER: u16 = 1;
    fn read_record(
        &mut self,
        _record_number: u16,
        _record_length: u16,
        _out: &mut [u8],
    ) -> Result<u8, errors::MbusError> {
        Ok(0)
    }
    fn write_record(
        &mut self,
        _record_number: u16,
        _record_length: u16,
        _data: &[u16],
    ) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

struct LogFile;
impl FileRecord for LogFile {
    const FILE_NUMBER: u16 = 2;
    fn read_record(
        &mut self,
        _record_number: u16,
        _record_length: u16,
        _out: &mut [u8],
    ) -> Result<u8, errors::MbusError> {
        Ok(0)
    }
    fn write_record(
        &mut self,
        _record_number: u16,
        _record_length: u16,
        _data: &[u16],
    ) -> Result<(), errors::MbusError> {
        Ok(())
    }
}

#[modbus_app(file_record(calibration, log))]
struct App {
    calibration: CalibrationFile,
    log: LogFile,
}

fn assert_file_record_handler<T: app::ServerFileRecordHandler>() {}

fn main() {
    assert_file_record_handler::<App>();
}
