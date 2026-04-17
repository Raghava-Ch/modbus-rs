mod common;
use common::{
    MockSerialTransport, MockTransport, build_request, build_serial_request, serial_rtu_config,
    tcp_config, unit_id,
};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ResilienceConfig;
use mbus_server::ServerServices;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerDiagnosticsHandler;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

struct Handles {
    calls: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Success(u8),
    AppError(MbusError),
}

struct Fc07App {
    mode: Mode,
    calls: Arc<AtomicUsize>,
}

fn make_app(mode: Mode) -> (Fc07App, Handles) {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Fc07App {
        mode,
        calls: Arc::clone(&calls),
    };
    let handles = Handles {
        calls: Arc::clone(&calls),
    };
    (app, handles)
}

impl ServerExceptionHandler for Fc07App {}
impl ServerCoilHandler for Fc07App {}
impl ServerDiscreteInputHandler for Fc07App {}
impl ServerHoldingRegisterHandler for Fc07App {}
impl ServerInputRegisterHandler for Fc07App {}
impl ServerFifoHandler for Fc07App {}
impl ServerFileRecordHandler for Fc07App {}
impl ServerDiagnosticsHandler for Fc07App {
    fn read_exception_status_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            Mode::Success(v) => Ok(v),
            Mode::AppError(err) => Err(err),
        }
    }
}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for Fc07App {}

fn run_once_serial(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: Fc07App) -> Vec<u8> {
    let sent_frames = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transport = MockSerialTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent_frames),
        connected: true,
    };
    let mut server = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server.poll();
    sent_frames
        .lock()
        .expect("sent frames mutex")
        .first()
        .cloned()
        .expect("server should send exactly one response")
}

fn run_once_tcp(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: Fc07App) -> Vec<u8> {
    let sent_frames = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: std::sync::Arc::clone(&sent_frames),
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
    sent_frames
        .lock()
        .expect("sent frames mutex")
        .first()
        .cloned()
        .expect("server should send exactly one response")
}

fn decode_exception(value: u8) -> ExceptionCode {
    match value {
        0x01 => ExceptionCode::IllegalFunction,
        0x02 => ExceptionCode::IllegalDataAddress,
        0x03 => ExceptionCode::IllegalDataValue,
        0x04 => ExceptionCode::ServerDeviceFailure,
        _ => panic!("unexpected exception code: {value:#04x}"),
    }
}

// Serial RTU ADU layout: [slave_addr][FC][data...][CRC_lo][CRC_hi]
// Response byte indices: [0]=slave_addr, [1]=FC, [2]=data/exception_code

#[test]
fn fc07_success_returns_single_status_byte() {
    let request = build_serial_request(1, unit_id(1), FunctionCode::ReadExceptionStatus, &[]);
    let (app, h) = make_app(Mode::Success(0b1010_0101));

    let response = run_once_serial(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[1], 0x07, "FC byte");
    assert_eq!(response[2], 0b1010_0101, "status byte");
}

#[test]
fn fc07_non_empty_request_payload_returns_exception_without_callback() {
    let request = build_serial_request(2, unit_id(1), FunctionCode::ReadExceptionStatus, &[0x01]);
    let (app, h) = make_app(Mode::Success(0x00));

    let response = run_once_serial(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[1], 0x87, "exception FC byte");
    assert_eq!(
        decode_exception(response[2]),
        ExceptionCode::IllegalDataAddress
    );
}

#[test]
fn fc07_app_invalid_address_maps_to_illegal_data_address() {
    let request = build_serial_request(3, unit_id(1), FunctionCode::ReadExceptionStatus, &[]);
    let (app, h) = make_app(Mode::AppError(MbusError::InvalidAddress));

    let response = run_once_serial(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[1], 0x87, "exception FC byte");
    assert_eq!(
        decode_exception(response[2]),
        ExceptionCode::IllegalDataAddress
    );
}

#[test]
fn fc07_app_unexpected_error_maps_to_server_device_failure() {
    let request = build_serial_request(4, unit_id(1), FunctionCode::ReadExceptionStatus, &[]);
    let (app, h) = make_app(Mode::AppError(MbusError::Unexpected));

    let response = run_once_serial(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[1], 0x87, "exception FC byte");
    assert_eq!(
        decode_exception(response[2]),
        ExceptionCode::ServerDeviceFailure
    );
}

#[test]
fn fc07_over_tcp_returns_illegal_function_exception() {
    // FC07 is serial-line-only; a TCP server must reject it with IllegalFunction.
    let request = build_request(5, unit_id(1), FunctionCode::ReadExceptionStatus, &[]);
    let (app, h) = make_app(Mode::Success(0xFF));

    let response = run_once_tcp(request, app);

    assert_eq!(
        h.calls.load(Ordering::SeqCst),
        0,
        "callback must not be invoked over TCP"
    );
    // TCP ADU: [0..1]=txn_id, [2..3]=proto, [4..5]=len, [6]=unit_id, [7]=FC, [8]=exception_code
    assert_eq!(response[7], 0x87, "exception FC byte");
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalFunction
    );
}
