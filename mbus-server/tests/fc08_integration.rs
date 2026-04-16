mod common;
use common::{build_serial_request, serial_rtu_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::{DiagnosticSubFunction, FunctionCode};
use mbus_core::transport::{
    ModbusConfig, SerialMode, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use mbus_server::ModbusAppHandler;
use mbus_server::ResilienceConfig;
use mbus_server::ServerServices;
use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug, Clone, Copy)]
enum Mode {
    Success(u16),
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
        _data: u16,
    ) -> Result<u16, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        match self.mode {
            Mode::Success(v) => Ok(v),
        }
    }
}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for Fc08App {}

#[derive(Debug)]
struct QueueSerialTransport {
    rx_queue: Arc<Mutex<VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>>>,
    sent_frames: Arc<Mutex<Vec<Vec<u8>>>>,
    connected: bool,
}

impl Transport for QueueSerialTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdSerial(SerialMode::Rtu);

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
        self.rx_queue
            .lock()
            .expect("rx_queue mutex poisoned")
            .pop_front()
            .ok_or(TransportError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

type TestServer = ServerServices<QueueSerialTransport, Fc08App, 8>;
type SharedRxQueue = Arc<Mutex<VecDeque<HVec<u8, MAX_ADU_FRAME_LEN>>>>;
type SharedSentFrames = Arc<Mutex<Vec<Vec<u8>>>>;

fn make_server(app: Fc08App) -> (TestServer, SharedRxQueue, SharedSentFrames) {
    let rx_queue = Arc::new(Mutex::new(VecDeque::new()));
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let transport = QueueSerialTransport {
        rx_queue: Arc::clone(&rx_queue),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };

    let server = ServerServices::new(
        transport,
        app,
        serial_rtu_config(),
        unit_id(1),
        ResilienceConfig::default(),
    );

    (server, rx_queue, sent_frames)
}

fn send_request(
    server: &mut TestServer,
    rx_queue: &SharedRxQueue,
    sent_frames: &SharedSentFrames,
    request: HVec<u8, MAX_ADU_FRAME_LEN>,
) -> Option<Vec<u8>> {
    let before = sent_frames
        .lock()
        .expect("sent_frames mutex poisoned")
        .len();
    rx_queue
        .lock()
        .expect("rx_queue mutex poisoned")
        .push_back(request);

    server.poll();

    let sent_frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    if sent_frames.len() > before {
        sent_frames.last().cloned()
    } else {
        None
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
    let (mut server, rx_queue, sent_frames) = make_server(app);

    let response = send_request(&mut server, &rx_queue, &sent_frames, req)
        .expect("0x0001 should produce a response");

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app should not be called for 0x0001"
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
    let (mut server, rx_queue, sent_frames) = make_server(app);

    let response = send_request(&mut server, &rx_queue, &sent_frames, req);

    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "app should not be called for 0x0004"
    );
    assert!(response.is_none(), "0x0004 should not send a response");
    assert_eq!(
        sent_frames.lock().unwrap().len(),
        0,
        "no response should be sent for 0x0004 per Modbus spec"
    );
}

#[test]
fn fc08_listen_only_mode_gates_other_functions() {
    let (app, calls) = make_app(Mode::Success(0));
    let (mut server, rx_queue, sent_frames) = make_server(app);

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

    let enter_response = send_request(&mut server, &rx_queue, &sent_frames, req_enter);
    assert!(
        enter_response.is_none(),
        "0x0004 should not send a response"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0, "0x0004 is stack-handled");

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

    let rc_response = send_request(&mut server, &rx_queue, &sent_frames, req_rc);

    assert_eq!(
        rc_response, None,
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

    let exit_response = send_request(&mut server, &rx_queue, &sent_frames, req_exit)
        .expect("0x0001 should send a response when exiting listen-only mode");

    assert_eq!(calls.load(Ordering::SeqCst), 0, "0x0001 is stack-handled");
    assert_eq!(exit_response[1], 0x08, "FC byte");
    assert_eq!(exit_response[2..4], [0x00, 0x01], "sub-function echo");
    assert_eq!(exit_response[4..6], [0x00, 0x00], "data echo");
    assert_eq!(
        sent_frames.lock().unwrap().len(),
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
    let (mut server, rx_queue, sent_frames) = make_server(app);

    let response = send_request(&mut server, &rx_queue, &sent_frames, req)
        .expect("0x0002 should produce a response");

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "app should be called for 0x0002"
    );
    assert_eq!(response[1], 0x08, "FC byte");
    assert_eq!(response[2..4], [0x00, 0x02], "sub-function echo");
    assert_eq!(response[4..6], [0x56, 0x78], "app result");
}
