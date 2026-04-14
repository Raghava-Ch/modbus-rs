// Allow items that aren't used in every test binary that includes this module.
#![allow(dead_code)]

// Shared test infrastructure for `mbus-server` integration tests.
// Each integration test binary in `tests/` declares `mod common;` to get
// access to [`MockTransport`] and the small helper functions below.

use heapless::Vec as HVec;
use mbus_core::data_unit::common::{compile_adu_frame, Pdu, MAX_ADU_FRAME_LEN};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use std::sync::{Arc, Mutex};

/// A minimal in-memory transport that serves one pre-built request frame and
/// captures all response frames emitted by [`ServerServices::poll`].
#[derive(Debug)]
pub struct MockTransport {
    pub next_rx: Option<HVec<u8, MAX_ADU_FRAME_LEN>>,
    pub sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    pub connected: bool,
}

impl Transport for MockTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        self.sent_frames
            .lock()
            .expect("sent_frames mutex poisoned")
            .push(adu.to_vec());
        Ok(())
    }

    fn recv(&mut self) -> Result<HVec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.next_rx.take().ok_or(TransportError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

/// Returns a [`UnitIdOrSlaveAddr`] for the given value, panicking on invalid input.
pub fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::new(v).expect("valid unit id")
}

/// Returns a standard TCP [`ModbusConfig`] pointing at `127.0.0.1:502`.
pub fn tcp_config() -> ModbusConfig {
    ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).expect("valid tcp config"))
}

/// Builds a complete TCP Modbus request ADU with the given function code and payload bytes.
pub fn build_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    payload: &[u8],
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::new(
        function_code,
        HVec::from_slice(payload).expect("request payload should fit in PDU"),
        payload.len() as u8,
    );
    compile_adu_frame(txn_id, unit.get(), pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}
