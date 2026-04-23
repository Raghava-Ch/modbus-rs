//! Example: using `fifo(...)` and `file_record(...)` selectors in `#[modbus_app]`.
//!
//! Run:
//! `cargo run -p modbus-rs --example fifo_file_record_demo --features server,network-tcp`

use mbus_core::{errors::MbusError, transport::UnitIdOrSlaveAddr};
use mbus_server::{
    FileRecord, FifoQueue, ServerFifoHandler, ServerFileRecordHandler, modbus_app,
};

#[derive(Default)]
struct TemperatureHistory;

impl FifoQueue for TemperatureHistory {
    const POINTER_ADDRESS: u16 = 0x0100;

    fn read_fifo_queue(&mut self, out: &mut [u8]) -> Result<u8, MbusError> {
        // Two values: 251, 252
        if out.len() < 6 {
            return Err(MbusError::BufferTooSmall);
        }
        out[0] = 0x00;
        out[1] = 0x02;
        out[2] = 0x00;
        out[3] = 0xFB;
        out[4] = 0x00;
        out[5] = 0xFC;
        Ok(6)
    }
}

#[derive(Default)]
struct AlarmFile;

impl FileRecord for AlarmFile {
    const FILE_NUMBER: u16 = 7;

    fn read_record(
        &mut self,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if record_number != 0 {
            return Err(MbusError::InvalidAddress);
        }
        if record_length != 2 || out.len() < 4 {
            return Err(MbusError::InvalidQuantity);
        }

        out[0] = 0x12;
        out[1] = 0x34;
        out[2] = 0xAB;
        out[3] = 0xCD;
        Ok(4)
    }

    fn write_record(
        &mut self,
        _record_number: u16,
        record_length: u16,
        data: &[u16],
    ) -> Result<(), MbusError> {
        if record_length as usize != data.len() {
            return Err(MbusError::InvalidQuantity);
        }
        Ok(())
    }
}

#[derive(Default)]
#[modbus_app(fifo(history), file_record(alarm_file))]
struct DemoApp {
    history: TemperatureHistory,
    alarm_file: AlarmFile,
}

fn main() -> Result<(), MbusError> {
    let mut app = DemoApp::default();
    let unit = UnitIdOrSlaveAddr::new(1)?;

    // FC18 route: pointer address selects the field from `fifo(...)`.
    let mut fifo_out = [0u8; 16];
    let fifo_bytes = ServerFifoHandler::read_fifo_queue_request(
        &mut app,
        1,
        unit,
        TemperatureHistory::POINTER_ADDRESS,
        &mut fifo_out,
    )?;
    assert_eq!(fifo_bytes, 6);
    assert_eq!(&fifo_out[..6], &[0x00, 0x02, 0x00, 0xFB, 0x00, 0xFC]);

    // Unknown pointer address -> macro-generated fallback.
    let fifo_err = ServerFifoHandler::read_fifo_queue_request(&mut app, 2, unit, 0x9999, &mut fifo_out)
        .expect_err("unknown FIFO pointer should fail");
    assert_eq!(fifo_err, MbusError::InvalidAddress);

    // FC14 route: file number selects the field from `file_record(...)`.
    let mut file_out = [0u8; 8];
    let read_bytes = ServerFileRecordHandler::read_file_record_request(
        &mut app,
        3,
        unit,
        AlarmFile::FILE_NUMBER,
        0,
        2,
        &mut file_out,
    )?;
    assert_eq!(read_bytes, 4);
    assert_eq!(&file_out[..4], &[0x12, 0x34, 0xAB, 0xCD]);

    ServerFileRecordHandler::write_file_record_request(
        &mut app,
        4,
        unit,
        AlarmFile::FILE_NUMBER,
        0,
        2,
        &[0xAAAA, 0x5555],
    )?;

    // Unknown file number -> macro-generated fallback.
    let file_err = ServerFileRecordHandler::read_file_record_request(
        &mut app,
        5,
        unit,
        999,
        0,
        1,
        &mut file_out,
    )
    .expect_err("unknown file number should fail");
    assert_eq!(file_err, MbusError::InvalidFunctionCode);

    println!("fifo(...) and file_record(...) selector example passed");
    Ok(())
}
