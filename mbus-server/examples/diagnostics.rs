//! Diagnostics-family server-side example.
//!
//! Run:
//! ```text
//! cargo run -p mbus-server --example diagnostics --features diagnostics
//! ```

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;

struct DiagnosticsApp {
    exception_status: u8,
}

impl DiagnosticsApp {
    fn new() -> Self {
        Self {
            exception_status: 0b0011_0101,
        }
    }
}

impl ModbusAppHandler for DiagnosticsApp {
    fn read_exception_status_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        Ok(self.exception_status)
    }

    fn report_server_id_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        let server_id = b"MBUS-SERVER";
        out_server_id[..server_id.len()].copy_from_slice(server_id);
        Ok((server_id.len() as u8, 0xFF))
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for DiagnosticsApp {}

fn main() {
    let mut app = DiagnosticsApp::new();
    let uid = UnitIdOrSlaveAddr::new(1).expect("valid unit id");

    let status = app
        .read_exception_status_request(42, uid)
        .expect("fc07 callback should succeed");

    let mut server_id_out = [0u8; 32];
    let (server_id_len, run_status) = app
        .report_server_id_request(43, uid, &mut server_id_out)
        .expect("fc11 callback should succeed");

    println!("FC07 status byte: {status:#010b} ({status:#04X})");
    println!(
        "FC11 server id: {} (run_status={run_status:#04X})",
        core::str::from_utf8(&server_id_out[..server_id_len as usize]).expect("valid utf8")
    );
    println!(
        "FC0B/FC0C are handled by the stack-side communication event tracker in ServerServices"
    );
}
