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
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{ResilienceConfig, ServerServices};
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerDiscreteInputHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ServerFifoHandler;
use mbus_server::ServerFileRecordHandler;
use mbus_server::ServerDiagnosticsHandler;

struct DiagnosticsExtApp;

impl ServerExceptionHandler for DiagnosticsExtApp {}
impl ServerCoilHandler for DiagnosticsExtApp {}
impl ServerDiscreteInputHandler for DiagnosticsExtApp {}
impl ServerHoldingRegisterHandler for DiagnosticsExtApp {}
impl ServerInputRegisterHandler for DiagnosticsExtApp {}
impl ServerFifoHandler for DiagnosticsExtApp {}
impl ServerFileRecordHandler for DiagnosticsExtApp {}
impl ServerDiagnosticsHandler for DiagnosticsExtApp {
    fn report_server_id_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        let id = b"MBUS";
        out_server_id[..id.len()].copy_from_slice(id);
        Ok((id.len() as u8, 0xFF))
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for DiagnosticsExtApp {}

fn run_once_serial(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: DiagnosticsExtApp) -> Vec<u8> {
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

fn run_once_tcp(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: DiagnosticsExtApp) -> Vec<u8> {
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

#[test]
fn fc0b_returns_status_word_and_event_count() {
    let request = build_serial_request(1, unit_id(1), FunctionCode::GetCommEventCounter, &[]);

    let response = run_once_serial(request, DiagnosticsExtApp);

    assert_eq!(response[1], 0x0B, "FC byte");
    let status_word = u16::from_be_bytes([response[2], response[3]]);
    let event_count = u16::from_be_bytes([response[4], response[5]]);
    assert_eq!(status_word, 0x8000);
    assert_eq!(event_count, 1);
}

#[test]
fn fc0c_returns_comm_event_log_payload() {
    let request = build_serial_request(2, unit_id(1), FunctionCode::GetCommEventLog, &[]);

    let response = run_once_serial(request, DiagnosticsExtApp);

    assert_eq!(response[1], 0x0C, "FC byte");
    assert_eq!(
        response[2], 7,
        "byte count = status(2)+event_count(2)+message_count(2)+events(1)"
    );

    let status_word = u16::from_be_bytes([response[3], response[4]]);
    let event_count = u16::from_be_bytes([response[5], response[6]]);
    let message_count = u16::from_be_bytes([response[7], response[8]]);
    let first_event = response[9];

    assert_eq!(status_word, 0x8000);
    assert_eq!(event_count, 1);
    assert_eq!(message_count, 1);
    assert_eq!(first_event, 0x04, "master initiated event");
}

#[test]
fn fc11_returns_server_id_and_run_status() {
    let request = build_serial_request(3, unit_id(1), FunctionCode::ReportServerId, &[]);

    let response = run_once_serial(request, DiagnosticsExtApp);

    assert_eq!(response[1], 0x11, "FC byte");
    assert_eq!(
        response[2], 5,
        "byte count should include server_id bytes + run indicator"
    );
    assert_eq!(&response[3..7], b"MBUS");
    assert_eq!(response[7], 0xFF, "run indicator");
}

#[test]
fn fc0b_over_tcp_returns_illegal_function_exception() {
    let request = build_request(4, unit_id(1), FunctionCode::GetCommEventCounter, &[]);

    let response = run_once_tcp(request, DiagnosticsExtApp);

    // TCP ADU: [0..1]=txn_id, [2..3]=proto, [4..5]=len, [6]=unit_id, [7]=FC, [8]=exception_code
    assert_eq!(response[7], 0x8B, "exception FC byte");
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalFunction
    );
}

#[test]
fn fc0c_over_tcp_returns_illegal_function_exception() {
    let request = build_request(5, unit_id(1), FunctionCode::GetCommEventLog, &[]);

    let response = run_once_tcp(request, DiagnosticsExtApp);

    // FC 0x0C | 0x80 = 0x8C
    assert_eq!(response[7], 0x8C, "exception FC byte");
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalFunction
    );
}

#[test]
fn fc11_over_tcp_returns_illegal_function_exception() {
    let request = build_request(6, unit_id(1), FunctionCode::ReportServerId, &[]);

    let response = run_once_tcp(request, DiagnosticsExtApp);

    // FC 0x11 | 0x80 = 0x91
    assert_eq!(response[7], 0x91, "exception FC byte");
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalFunction
    );
}
