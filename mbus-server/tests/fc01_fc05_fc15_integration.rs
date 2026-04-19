#![cfg(feature = "coils")]

mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ResilienceConfig;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiagnosticsHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerServices;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Fc05Last = Arc<Mutex<Option<(u16, bool)>>>;
type Fc15Last = Arc<Mutex<Option<(u16, u16, Vec<u8>)>>>;

#[derive(Debug, Clone, Copy)]
enum CoilMode {
    Success,
    AppError(MbusError),
}

#[derive(Debug)]
struct CoilApp {
    mode_fc01: CoilMode,
    mode_fc05: CoilMode,
    mode_fc15: CoilMode,
    fc01_calls: Arc<AtomicUsize>,
    fc05_calls: Arc<AtomicUsize>,
    fc15_calls: Arc<AtomicUsize>,
    fc05_last: Fc05Last,
    fc15_last: Fc15Last,
}

impl ServerExceptionHandler for CoilApp {}

impl ServerDiscreteInputHandler for CoilApp {}

impl ServerHoldingRegisterHandler for CoilApp {}

impl ServerInputRegisterHandler for CoilApp {}

impl ServerFifoHandler for CoilApp {}

impl ServerFileRecordHandler for CoilApp {}

impl ServerDiagnosticsHandler for CoilApp {}

impl ServerCoilHandler for CoilApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.fc01_calls.fetch_add(1, Ordering::SeqCst);
        match self.mode_fc01 {
            CoilMode::Success => {
                // Deterministic pattern used by assertions below.
                let needed = (quantity as usize).div_ceil(8);
                out[..needed].fill(0);
                if needed > 0 {
                    out[0] = 0b0000_0101;
                }
                Ok(needed as u8)
            }
            CoilMode::AppError(error) => Err(error),
        }
    }

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        self.fc05_calls.fetch_add(1, Ordering::SeqCst);
        *self.fc05_last.lock().expect("fc05 mutex poisoned") = Some((address, value));

        match self.mode_fc05 {
            CoilMode::Success => Ok(()),
            CoilMode::AppError(error) => Err(error),
        }
    }

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        self.fc15_calls.fetch_add(1, Ordering::SeqCst);
        *self.fc15_last.lock().expect("fc15 mutex poisoned") =
            Some((starting_address, quantity, values.to_vec()));

        match self.mode_fc15 {
            CoilMode::Success => Ok(()),
            CoilMode::AppError(error) => Err(error),
        }
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for CoilApp {}

fn decode_exception_code(value: u8) -> ExceptionCode {
    match value {
        0x01 => ExceptionCode::IllegalFunction,
        0x02 => ExceptionCode::IllegalDataAddress,
        0x03 => ExceptionCode::IllegalDataValue,
        0x04 => ExceptionCode::ServerDeviceFailure,
        _ => panic!("unexpected exception code: {value:#04x}"),
    }
}

fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: CoilApp) -> Vec<u8> {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };

    let mut server = ServerServices::new(
        transport,
        app,
        tcp_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server.poll();

    let frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(frames.len(), 1, "server should emit exactly one response");
    frames[0].clone()
}

#[derive(Debug)]
struct Probe {
    fc01_calls: Arc<AtomicUsize>,
    fc05_calls: Arc<AtomicUsize>,
    fc15_calls: Arc<AtomicUsize>,
    fc05_last: Fc05Last,
    fc15_last: Fc15Last,
}

fn make_app(mode_fc01: CoilMode, mode_fc05: CoilMode, mode_fc15: CoilMode) -> (CoilApp, Probe) {
    let fc01_calls = Arc::new(AtomicUsize::new(0));
    let fc05_calls = Arc::new(AtomicUsize::new(0));
    let fc15_calls = Arc::new(AtomicUsize::new(0));
    let fc05_last = Arc::new(Mutex::new(None));
    let fc15_last = Arc::new(Mutex::new(None));

    let app = CoilApp {
        mode_fc01,
        mode_fc05,
        mode_fc15,
        fc01_calls: Arc::clone(&fc01_calls),
        fc05_calls: Arc::clone(&fc05_calls),
        fc15_calls: Arc::clone(&fc15_calls),
        fc05_last: Arc::clone(&fc05_last),
        fc15_last: Arc::clone(&fc15_last),
    };

    let probe = Probe {
        fc01_calls,
        fc05_calls,
        fc15_calls,
        fc05_last,
        fc15_last,
    };

    (app, probe)
}

#[test]
fn fc01_success_returns_packed_coil_bytes() {
    let request = build_request(
        21,
        unit_id(1),
        FunctionCode::ReadCoils,
        &[0x00, 0x13, 0x00, 0x03],
    );
    let (app, probe) = make_app(
        CoilMode::Success,
        CoilMode::AppError(MbusError::InvalidFunctionCode),
        CoilMode::AppError(MbusError::InvalidFunctionCode),
    );

    let response = run_once(request, app);

    assert_eq!(probe.fc01_calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x01);
    assert_eq!(response[8], 1);
    assert_eq!(response[9], 0b0000_0101);
}

#[test]
fn fc01_invalid_quantity_returns_exception_before_callback() {
    let request = build_request(
        22,
        unit_id(1),
        FunctionCode::ReadCoils,
        &[0x00, 0x13, 0x00, 0x00],
    );
    let (app, probe) = make_app(CoilMode::Success, CoilMode::Success, CoilMode::Success);

    let response = run_once(request, app);

    assert_eq!(probe.fc01_calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x81);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc05_success_echoes_address_and_raw_value() {
    let request = build_request(
        23,
        unit_id(1),
        FunctionCode::WriteSingleCoil,
        &[0x00, 0x2A, 0xFF, 0x00],
    );
    let (app, probe) = make_app(CoilMode::Success, CoilMode::Success, CoilMode::Success);

    let response = run_once(request, app);

    assert_eq!(probe.fc05_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *probe.fc05_last.lock().expect("fc05 mutex poisoned"),
        Some((0x002A, true))
    );
    assert_eq!(response[7], 0x05);
    assert_eq!(&response[8..12], &[0x00, 0x2A, 0xFF, 0x00]);
}

#[test]
fn fc05_invalid_raw_value_returns_exception_before_callback() {
    let request = build_request(
        24,
        unit_id(1),
        FunctionCode::WriteSingleCoil,
        &[0x00, 0x2A, 0x12, 0x34],
    );
    let (app, probe) = make_app(CoilMode::Success, CoilMode::Success, CoilMode::Success);

    let response = run_once(request, app);

    assert_eq!(probe.fc05_calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x85);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc15_success_writes_packed_values_and_echoes_window() {
    let request = build_request(
        25,
        unit_id(1),
        FunctionCode::WriteMultipleCoils,
        &[0x00, 0x30, 0x00, 0x09, 0x02, 0x55, 0x01],
    );
    let (app, probe) = make_app(CoilMode::Success, CoilMode::Success, CoilMode::Success);

    let response = run_once(request, app);

    assert_eq!(probe.fc15_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *probe.fc15_last.lock().expect("fc15 mutex poisoned"),
        Some((0x0030, 9, vec![0x55, 0x01]))
    );
    assert_eq!(response[7], 0x0F);
    assert_eq!(&response[8..12], &[0x00, 0x30, 0x00, 0x09]);
}

#[test]
fn fc15_invalid_byte_count_returns_exception_before_callback() {
    let request = build_request(
        26,
        unit_id(1),
        FunctionCode::WriteMultipleCoils,
        &[0x00, 0x30, 0x00, 0x09, 0x01, 0x55],
    );
    let (app, probe) = make_app(CoilMode::Success, CoilMode::Success, CoilMode::Success);

    let response = run_once(request, app);

    assert_eq!(probe.fc15_calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x8F);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
}
