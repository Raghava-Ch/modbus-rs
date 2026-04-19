//! End-to-end tests wiring `ForwardingApp<A>` into `ServerServices<MockTransport, ForwardingApp<A>>`.
//!
//! These tests exercise the full frame → parse → dispatch → forward → respond cycle
//! and catch any trait-bound mismatch or coherence issue that only surfaces when
//! `ServerServices` holds the `ForwardingApp` as its `APP` generic parameter.
#![cfg(all(feature = "coils", feature = "holding-registers"))]

mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiagnosticsHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{
    ForwardingApp, ModbusAppAccess, ResilienceConfig, ServerExceptionHandler, ServerServices,
};
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Concrete app: holds two registers and four coils.
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct DemoApp {
    reg0: u16,
    reg1: u16,
    /// Bit N = coil N (low four bits used).
    coils: u8,
    #[cfg(feature = "traffic")]
    traffic_rx_frames: usize,
    #[cfg(feature = "traffic")]
    traffic_tx_frames: usize,
    #[cfg(feature = "traffic")]
    traffic_rx_errors: usize,
    #[cfg(feature = "traffic")]
    traffic_tx_errors: usize,
}

impl ServerExceptionHandler for DemoApp {}

impl ServerDiscreteInputHandler for DemoApp {}

impl ServerInputRegisterHandler for DemoApp {}

impl ServerFifoHandler for DemoApp {}

impl ServerFileRecordHandler for DemoApp {}

impl ServerDiagnosticsHandler for DemoApp {}

impl ServerHoldingRegisterHandler for DemoApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        match (address, quantity) {
            (0, 1) => {
                out[0] = (self.reg0 >> 8) as u8;
                out[1] = self.reg0 as u8;
                Ok(2)
            }
            (0, 2) => {
                out[0] = (self.reg0 >> 8) as u8;
                out[1] = self.reg0 as u8;
                out[2] = (self.reg1 >> 8) as u8;
                out[3] = self.reg1 as u8;
                Ok(4)
            }
            _ => Err(MbusError::InvalidAddress),
        }
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        match address {
            0 => {
                self.reg0 = value;
                Ok(())
            }
            1 => {
                self.reg1 = value;
                Ok(())
            }
            _ => Err(MbusError::InvalidAddress),
        }
    }
}

impl ServerCoilHandler for DemoApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        if address != 0 || quantity > 4 {
            return Err(MbusError::InvalidAddress);
        }
        out[0] = self.coils & 0x0F;
        Ok(1)
    }

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        if address >= 4 {
            return Err(MbusError::InvalidAddress);
        }
        if value {
            self.coils |= 1 << address;
        } else {
            self.coils &= !(1 << address);
        }
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for DemoApp {
    fn on_rx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
        self.traffic_rx_frames += 1;
    }

    fn on_tx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
        self.traffic_tx_frames += 1;
    }

    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
        self.traffic_rx_errors += 1;
    }

    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
        self.traffic_tx_errors += 1;
    }
}

// ---------------------------------------------------------------------------
// Mutex-based access wrapper.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MutexAccess {
    inner: Arc<Mutex<DemoApp>>,
}

impl ModbusAppAccess for MutexAccess {
    type App = DemoApp;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        let mut app = self.inner.lock().expect("mutex poisoned");
        f(&mut app)
    }
}

// ---------------------------------------------------------------------------
// Helpers.
// ---------------------------------------------------------------------------

/// Builds a `ServerServices<MockTransport, ForwardingApp<MutexAccess>>` and runs one poll.
/// Returns the single response frame the server emits.
fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, state: Arc<Mutex<DemoApp>>) -> Vec<u8> {
    let sent_frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };

    let access = MutexAccess {
        inner: Arc::clone(&state),
    };
    let fwd_app: ForwardingApp<MutexAccess> = ForwardingApp::new(access);

    // KEY: ServerServices<MockTransport, ForwardingApp<MutexAccess>>
    let mut server: ServerServices<MockTransport, ForwardingApp<MutexAccess>> = ServerServices::new(
        transport,
        fwd_app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    server.poll();

    let frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(frames.len(), 1, "server should emit exactly one response");
    frames[0].clone()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[test]
fn fc06_write_then_fc03_read_via_forwarding_app() {
    let state = Arc::new(Mutex::new(DemoApp::default()));

    // FC06: write 0xABCD to register 0.
    let write_req = build_request(
        10,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x00, 0xAB, 0xCD],
    );
    let write_resp = run_once(write_req, Arc::clone(&state));
    // Echo response: func=0x06, addr hi/lo, value hi/lo.
    assert_eq!(write_resp[7], 0x06, "FC06 echo response function code");
    assert_eq!(&write_resp[8..12], &[0x00, 0x00, 0xAB, 0xCD]);

    // FC03: read back register 0 — should see the value just written.
    let read_req = build_request(
        11,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &[0x00, 0x00, 0x00, 0x01],
    );
    let read_resp = run_once(read_req, Arc::clone(&state));
    // Response: func=0x03, byte_count=2, value hi/lo.
    assert_eq!(read_resp[7], 0x03, "FC03 response function code");
    assert_eq!(read_resp[8], 2, "byte count");
    assert_eq!(&read_resp[9..11], &[0xAB, 0xCD]);
}

#[test]
fn fc05_write_then_fc01_read_via_forwarding_app() {
    let state = Arc::new(Mutex::new(DemoApp::default()));

    // FC05: set coil 1 (ON = 0xFF00).
    let write_req = build_request(
        20,
        unit_id(1),
        FunctionCode::WriteSingleCoil,
        &[0x00, 0x01, 0xFF, 0x00],
    );
    let write_resp = run_once(write_req, Arc::clone(&state));
    // Echo response: func=0x05, addr hi/lo, value hi/lo.
    assert_eq!(write_resp[7], 0x05, "FC05 echo response function code");
    assert_eq!(&write_resp[8..12], &[0x00, 0x01, 0xFF, 0x00]);

    // FC01: read coils 0–3; coil 1 should be set.
    let read_req = build_request(
        21,
        unit_id(1),
        FunctionCode::ReadCoils,
        &[0x00, 0x00, 0x00, 0x04],
    );
    let read_resp = run_once(read_req, Arc::clone(&state));
    // Response: func=0x01, byte_count=1, packed coils.
    assert_eq!(read_resp[7], 0x01, "FC01 response function code");
    assert_eq!(read_resp[8], 1, "byte count");
    // bit 1 set → 0b0000_0010
    assert_eq!(read_resp[9] & 0x0F, 0b0000_0010);
}

#[test]
fn forwarding_app_error_propagates_to_exception_response() {
    // Pre-seed register 0 so FC03 for address 99 returns InvalidAddress.
    let state = Arc::new(Mutex::new(DemoApp::default()));

    // FC03: request address 99 — the app returns InvalidAddress.
    let req = build_request(
        30,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &[0x00, 0x63, 0x00, 0x01], // address=99, quantity=1
    );
    let resp = run_once(req, state);

    // Exception response: func=0x83, exception_code=0x02 (Illegal Data Address).
    assert_eq!(resp[7], 0x83, "exception response function code");
    assert_eq!(resp[8], 0x02, "exception code should be IllegalDataAddress");
}

#[test]
fn forwarding_app_shared_state_is_visible_across_separate_server_instances() {
    // Same Arc<Mutex<DemoApp>> passed to two separate server instances,
    // proving the Mutex correctly shares state between sequential polls.
    let state = Arc::new(Mutex::new(DemoApp::default()));

    // Write 0x1234 to reg0 via first server instance.
    let write_req = build_request(
        40,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x00, 0x12, 0x34],
    );
    run_once(write_req, Arc::clone(&state));

    // Read it back via a second, independently constructed server instance.
    let read_req = build_request(
        41,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &[0x00, 0x00, 0x00, 0x01],
    );
    let read_resp = run_once(read_req, Arc::clone(&state));

    assert_eq!(read_resp[7], 0x03);
    assert_eq!(&read_resp[9..11], &[0x12, 0x34]);
}

#[cfg(feature = "traffic")]
#[test]
fn forwarding_app_traffic_callbacks_on_success_path() {
    let state = Arc::new(Mutex::new(DemoApp::default()));

    let req = build_request(
        40,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x00, 0x12, 0x34],
    );

    let _resp = run_once(req, Arc::clone(&state));
    let app = state.lock().expect("state mutex poisoned");

    assert_eq!(app.traffic_rx_frames, 1);
    assert_eq!(app.traffic_tx_frames, 1);
    assert_eq!(app.traffic_rx_errors, 0);
    assert_eq!(app.traffic_tx_errors, 0);
}

#[cfg(feature = "traffic")]
#[test]
fn forwarding_app_traffic_callbacks_on_exception_path() {
    let state = Arc::new(Mutex::new(DemoApp::default()));

    let req = build_request(
        41,
        unit_id(1),
        FunctionCode::ReadHoldingRegisters,
        &[0x00, 0x63, 0x00, 0x01],
    );

    let _resp = run_once(req, Arc::clone(&state));
    let app = state.lock().expect("state mutex poisoned");

    assert_eq!(app.traffic_rx_frames, 1);
    assert_eq!(app.traffic_tx_frames, 1);
    assert_eq!(app.traffic_rx_errors, 1);
    assert_eq!(app.traffic_tx_errors, 0);
}
