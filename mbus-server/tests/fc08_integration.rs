mod common;
use common::{MockSerialTransport, build_serial_request, serial_rtu_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::{DiagnosticSubFunction, FunctionCode};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
use mbus_server::ResilienceConfig;
use mbus_server::ServerServices;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy)]
enum Mode {
    Success(u16),
    AppError(MbusError),
}

struct Fc08App {
    mode: Mode,
    calls: Arc<AtomicUsize>,
}

fn make_app(mode: Mode) -> (Fc08App, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let app = Fc08App {
        mode,
        calls: Arc::clone(&calls),
    };
    (app, calls)
}

impl ModbusAppHandler for Fc08App {
    #[cfg(feature = "diagnostics")]
    fn diagnostics_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _sub_function: DiagnosticSubFunction,
        data: u16,
    ) -> Result<u16, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            Mode::Success(v) => Ok(v),
            Mode::AppError(err) => Err(err),
        }
    }
}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for Fc08App {}

fn run_once_serial(
    request: HVec<u8, MAX_ADU_FRAME_LEN>,
    app: Fc08App,
) -> (Vec<u8>, ServerServices<MockSerialTransport, Fc08App, 8>) {
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
    let response = sent_frames
        .lock()
        .expect("sent frames mutex")
        .first()
        .cloned()
        .expect("server should send exactly one response");
    (response, server)
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
// Response byte indices: [0]=slave_addr, [1]=FC, [2..6]=data (4 bytes for FC08)

#[test]
fn fc08_0x0001_restart_communications_returns_echo() {
    // Sub-function 0x0001 (Restart Communications Option) with data=0x0000
    let mut request = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request.extend_from_slice(&[0x00, 0x01]).unwrap(); // sub-function 0x0001
    request.extend_from_slice(&[0x00, 0x00]).unwrap(); // data = 0x0000

    let req = build_serial_request(1, unit_id(1), FunctionCode::Diagnostics, request.as_slice());
    let (app, calls) = make_app(Mode::Success(0xABCD)); // app is not called for 0x0001

    let (response, server) = run_once_serial(req, app);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app should not be called for 0x0001"
    );
    assert!(
        !server.listen_only_mode,
        "listen-only mode should be cleared"
    );
    assert_eq!(response[1], 0x08, "FC byte");
    // Response echoes sub-function (0x0001) and data (0x0000)
    assert_eq!(response[2..4], [0x00, 0x01], "sub-function echo");
    assert_eq!(response[4..6], [0x00, 0x00], "data echo");
}

#[test]
fn fc08_0x0004_force_listen_only_mode_no_response() {
    // Sub-function 0x0004 (Force Listen Only Mode) with data=0x0000
    let mut request = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request.extend_from_slice(&[0x00, 0x04]).unwrap(); // sub-function 0x0004
    request.extend_from_slice(&[0x00, 0x00]).unwrap(); // data = 0x0000

    let req = build_serial_request(1, unit_id(1), FunctionCode::Diagnostics, request.as_slice());
    let (app, calls) = make_app(Mode::Success(0x1234));

    let transport = MockSerialTransport {
        next_rx: Some(req),
        sent_frames: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
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

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app should not be called for 0x0004"
    );
    assert!(server.listen_only_mode, "listen-only mode should be set");
    assert_eq!(
        server.transport.sent_frames.lock().unwrap().len(),
        0,
        "no response should be sent for 0x0004 per Modbus spec"
    );
}

#[test]
fn fc08_listen_only_mode_gates_other_functions() {
    // 1. Enter listen-only mode
    let mut request_enter = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request_enter.extend_from_slice(&[0x00, 0x04]).unwrap();
    request_enter.extend_from_slice(&[0x00, 0x00]).unwrap();

    let req_enter = build_serial_request(
        1,
        unit_id(1),
        FunctionCode::Diagnostics,
        request_enter.as_slice(),
    );

    let transport1 = MockSerialTransport {
        next_rx: Some(req_enter),
        sent_frames: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        connected: true,
    };
    let app = Fc08App {
        mode: Mode::Success(0),
        calls: Arc::new(AtomicUsize::new(0)),
    };
    let mut server1 = ServerServices::new(
        transport1,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server1.poll();
    assert!(server1.listen_only_mode);

    // 2. Try to send a ReadCoils (FC01) request while in listen-only mode
    // It should be silently discarded (no response)
    let mut request_rc = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request_rc.extend_from_slice(&[0x00, 0x00]).unwrap(); // coil address 0
    request_rc.extend_from_slice(&[0x00, 0x01]).unwrap(); // quantity 1

    let req_rc = build_serial_request(
        2,
        unit_id(1),
        FunctionCode::ReadCoils,
        request_rc.as_slice(),
    );

    let transport2 = MockSerialTransport {
        next_rx: Some(req_rc),
        sent_frames: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        connected: true,
    };
    let (app2, _) = make_app(Mode::Success(0xFFFF));
    let mut server2 = ServerServices::new(
        transport2,
        app2,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server2.listen_only_mode = true;
    server2.poll();

    assert_eq!(
        server2.transport.sent_frames.lock().unwrap().len(),
        0,
        "listen-only mode should silently discard non-diagnostics requests"
    );

    // 3. Now send FC08 with 0x0001 to exit listen-only mode
    let mut request_exit = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request_exit.extend_from_slice(&[0x00, 0x01]).unwrap();
    request_exit.extend_from_slice(&[0x00, 0x00]).unwrap();

    let req_exit = build_serial_request(
        3,
        unit_id(1),
        FunctionCode::Diagnostics,
        request_exit.as_slice(),
    );

    let transport3 = MockSerialTransport {
        next_rx: Some(req_exit),
        sent_frames: std::sync::Arc::new(std::sync::Mutex::new(Vec::new())),
        connected: true,
    };
    let (app3, _) = make_app(Mode::Success(0xFFFF));
    let mut server3 = ServerServices::new(
        transport3,
        app3,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );
    server3.listen_only_mode = true;
    server3.poll();

    assert!(
        !server3.listen_only_mode,
        "0x0001 should clear listen-only mode"
    );
    assert_eq!(
        server3.transport.sent_frames.lock().unwrap().len(),
        1,
        "0x0001 should send a response when exiting listen-only mode"
    );
}

#[test]
fn fc08_other_subfunctions_forward_to_app() {
    // Sub-function 0x0002 (Return Diagnostic Register) — not stack-handled, delegates to app
    let mut request = HVec::<u8, MAX_ADU_FRAME_LEN>::new();
    request.extend_from_slice(&[0x00, 0x02]).unwrap(); // sub-function 0x0002
    request.extend_from_slice(&[0x00, 0x00]).unwrap(); // data = 0x0000

    let req = build_serial_request(1, unit_id(1), FunctionCode::Diagnostics, request.as_slice());
    let (app, calls) = make_app(Mode::Success(0x5678));

    let (response, _) = run_once_serial(req, app);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "app should be called for 0x0002"
    );
    assert_eq!(response[1], 0x08, "FC byte");
    assert_eq!(response[2..4], [0x00, 0x02], "sub-function echo");
    assert_eq!(response[4..6], [0x56, 0x78], "app result");
}
