//! Minimal File Record (FC14/FC15) server-side example.
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example file_record --features file-record
//! ```

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

struct FileStore {
    file1: [u16; 32],
}

impl FileStore {
    fn new() -> Self {
        let mut file1 = [0u16; 32];
        for (i, slot) in file1.iter_mut().enumerate() {
            *slot = 0x1000 + i as u16;
        }
        Self { file1 }
    }
}

impl ModbusAppHandler for FileStore {
    fn read_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if file_number != 1 {
            return Err(MbusError::InvalidAddress);
        }

        let start = record_number as usize;
        let len = record_length as usize;
        if start.checked_add(len).is_none() || start + len > self.file1.len() {
            return Err(MbusError::InvalidAddress);
        }

        for i in 0..len {
            let value = self.file1[start + i];
            out[i * 2] = (value >> 8) as u8;
            out[i * 2 + 1] = value as u8;
        }

        Ok((record_length * 2) as u8)
    }

    fn write_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: &[u16],
    ) -> Result<(), MbusError> {
        if file_number != 1 {
            return Err(MbusError::InvalidAddress);
        }
        if record_data.len() != record_length as usize {
            return Err(MbusError::InvalidByteCount);
        }

        let start = record_number as usize;
        let len = record_length as usize;
        if start.checked_add(len).is_none() || start + len > self.file1.len() {
            return Err(MbusError::InvalidAddress);
        }

        for (i, &v) in record_data.iter().enumerate() {
            self.file1[start + i] = v;
        }

        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for FileStore {}

fn main() {
    let mut app = FileStore::new();
    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");

    // Simulate FC15 write sub-request: file=1, record=4, values=[0xABCD, 0xBCDE].
    app.write_file_record_request(1, uid, 1, 4, 2, &[0xABCD, 0xBCDE])
        .expect("write should succeed");

    // Simulate FC14 read sub-request over the same range.
    let mut out = [0u8; 16];
    let n = app
        .read_file_record_request(2, uid, 1, 4, 2, &mut out)
        .expect("read should succeed");

    println!("read bytes: {n}");
    for i in 0..2 {
        let v = u16::from_be_bytes([out[i * 2], out[i * 2 + 1]]);
        println!("record[{i}] = {v:#06X}");
    }
}
