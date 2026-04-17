mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// --------------------------------------------------------------------------
// Shared state handles
// --------------------------------------------------------------------------

type FifoCapture = Arc<Mutex<Option<u16>>>;

struct Fc18Handles {
    calls: Arc<AtomicUsize>,
    last_pointer: FifoCapture,
}

// --------------------------------------------------------------------------
// Test app
// --------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Mode {
    /// Return a FIFO with the given values.
    Success(Vec<u16>),
    /// Return an application-level error.
    AppError(MbusError),
    /// Write a deliberate bad byte count (returned len != 2 + count*2).
    BadByteCount,
    /// Write fifo_count > 31 (to trigger server-side limit check).
    ExceedMaxCount,
}

struct Fc18App {
    mode: Mode,
    calls: Arc<AtomicUsize>,
    last_pointer: FifoCapture,
}

fn make_app(mode: Mode) -> (Fc18App, Fc18Handles) {
    let calls = Arc::new(AtomicUsize::new(0));
    let last_pointer = Arc::new(Mutex::new(None));
    let app = Fc18App {
        mode: mode.clone(),
        calls: Arc::clone(&calls),
        last_pointer: Arc::clone(&last_pointer),
    };
    let handles = Fc18Handles {
        calls: Arc::clone(&calls),
        last_pointer: Arc::clone(&last_pointer),
    };
    (app, handles)
}

impl ServerExceptionHandler for Fc18App {}
impl ServerCoilHandler for Fc18App {}
impl ServerDiscreteInputHandler for Fc18App {}
impl ServerHoldingRegisterHandler for Fc18App {}
impl ServerInputRegisterHandler for Fc18App {}
impl ServerFifoHandler for Fc18App {
    fn read_fifo_queue_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        *self.last_pointer.lock().expect("pointer mutex") = Some(pointer_address);

        match &self.mode {
            Mode::Success(values) => {
                let count = values.len() as u16;
                out[0] = (count >> 8) as u8;
                out[1] = count as u8;
                for (i, &v) in values.iter().enumerate() {
                    out[2 + i * 2] = (v >> 8) as u8;
                    out[2 + i * 2 + 1] = v as u8;
                }
                Ok(2 + count as u8 * 2)
            }
            Mode::AppError(err) => Err(*err),
            Mode::BadByteCount => {
                // Write fifo_count = 2, but only write 1 value (3 bytes instead of 6)
                out[0] = 0x00;
                out[1] = 0x02; // claims 2 entries
                out[2] = 0xAB;
                out[3] = 0xCD; // only 1 value written (4 bytes total, expected 6)
                Ok(4) // incorrect: should be 6 for count=2
            }
            Mode::ExceedMaxCount => {
                // Write fifo_count = 32 (exceeds FC18_MAX_FIFO_COUNT = 31)
                let count: u16 = 32;
                out[0] = (count >> 8) as u8;
                out[1] = count as u8;
                for i in 0..count as usize {
                    out[2 + i * 2] = 0x00;
                    out[2 + i * 2 + 1] = 0x00;
                }
                Ok(2 + count as u8 * 2)
            }
        }
    }
}
impl ServerFileRecordHandler for Fc18App {}
impl ServerDiagnosticsHandler for Fc18App {}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for Fc18App {}

// --------------------------------------------------------------------------
// Test helpers
// --------------------------------------------------------------------------

fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: Fc18App) -> Vec<u8> {
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
    sent_frames
        .lock()
        .expect("sent_frames mutex")
        .first()
        .cloned()
        .expect("server should send exactly one frame")
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

/// Builds a raw FC18 request payload: just the pointer address (2 bytes).
fn fc18_payload(pointer_address: u16) -> Vec<u8> {
    pointer_address.to_be_bytes().to_vec()
}

// --------------------------------------------------------------------------
// TCP ADU layout for FC18:
//   [0..5] MBAP header  (txn_id(2) + protocol(2) + length(2))
//   [6]    unit_id
//   [7]    function_code (0x18)
//   [8..9] byte_count   (u16 BE) = 2 + fifo_count * 2
//   [10..11] fifo_count (u16 BE)
//   [12..] values       (fifo_count * 2 bytes)
// --------------------------------------------------------------------------

#[test]
fn fc18_non_empty_fifo_returns_correct_values() {
    let values = vec![0x1111u16, 0x2222, 0x3333];
    let payload = fc18_payload(0x00AA);
    let request = build_request(1, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::Success(values.clone()));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(*h.last_pointer.lock().expect("pointer mutex"), Some(0x00AA));
    // FC byte
    assert_eq!(response[7], 0x18);
    // byte_count = 2 + 3*2 = 8
    let byte_count = u16::from_be_bytes([response[8], response[9]]);
    assert_eq!(byte_count, 8);
    // fifo_count = 3
    let fifo_count = u16::from_be_bytes([response[10], response[11]]);
    assert_eq!(fifo_count, 3);
    // values
    assert_eq!(&response[12..14], &[0x11, 0x11]);
    assert_eq!(&response[14..16], &[0x22, 0x22]);
    assert_eq!(&response[16..18], &[0x33, 0x33]);
}

#[test]
fn fc18_empty_fifo_returns_zero_count() {
    let payload = fc18_payload(0x0010);
    let request = build_request(2, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::Success(vec![]));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    // FC byte
    assert_eq!(response[7], 0x18);
    // byte_count = 2 + 0*2 = 2
    let byte_count = u16::from_be_bytes([response[8], response[9]]);
    assert_eq!(byte_count, 2);
    // fifo_count = 0
    let fifo_count = u16::from_be_bytes([response[10], response[11]]);
    assert_eq!(fifo_count, 0);
    // no value bytes follow
    assert_eq!(response.len(), 12);
}

#[test]
fn fc18_pointer_address_is_forwarded_to_app() {
    let payload = fc18_payload(0x1234);
    let request = build_request(3, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::Success(vec![0xABCD]));

    let _ = run_once(request, app);

    assert_eq!(*h.last_pointer.lock().expect("pointer mutex"), Some(0x1234));
}

#[test]
fn fc18_app_returns_invalid_address_emits_exception() {
    let payload = fc18_payload(0x0000);
    let request = build_request(4, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::AppError(MbusError::InvalidAddress));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x98);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataAddress
    );
}

#[test]
fn fc18_app_returns_unexpected_error_emits_server_failure() {
    let payload = fc18_payload(0x0000);
    let request = build_request(5, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::AppError(MbusError::Unexpected));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x98);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::ServerDeviceFailure
    );
}

#[test]
fn fc18_fifo_count_exceeds_31_returns_illegal_data_value_no_callback_skipped() {
    // The ExceedMaxCount mode writes count=32 in the buffer. The server
    // should catch it and emit IllegalDataValue. The app callback IS called
    // (the server can't know BEFORE the call); the exception fires after.
    let payload = fc18_payload(0x0000);
    let request = build_request(6, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::ExceedMaxCount);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x98);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc18_mismatched_byte_count_returns_illegal_data_value() {
    // BadByteCount mode writes fifo_count=2 but only 4 bytes (expected 6).
    let payload = fc18_payload(0x0000);
    let request = build_request(7, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::BadByteCount);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x98);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc18_request_with_wrong_payload_length_returns_exception_no_callback() {
    // Send a request with 4 bytes of payload instead of 2 → parse fails.
    let bad_payload: Vec<u8> = vec![0x00, 0x10, 0x00, 0x00]; // 4 bytes, FC18 expects 2
    let request = build_request(8, unit_id(1), FunctionCode::ReadFifoQueue, &bad_payload);
    let (app, h) = make_app(Mode::Success(vec![]));

    let response = run_once(request, app);

    // No app callback — parse failed before dispatch
    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x98);
}

#[test]
fn fc18_max_31_entries_succeeds() {
    // Exactly 31 entries — should succeed without exception.
    let values: Vec<u16> = (0..31).map(|i| i as u16).collect();
    let payload = fc18_payload(0x0001);
    let request = build_request(9, unit_id(1), FunctionCode::ReadFifoQueue, &payload);
    let (app, h) = make_app(Mode::Success(values));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x18);
    let byte_count = u16::from_be_bytes([response[8], response[9]]);
    assert_eq!(byte_count, 2 + 31 * 2); // 64
    let fifo_count = u16::from_be_bytes([response[10], response[11]]);
    assert_eq!(fifo_count, 31);
}
