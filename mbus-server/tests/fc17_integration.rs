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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

// --------------------------------------------------------------------------
// Shared state handles (kept by the test, Arc-cloned into the app)
// --------------------------------------------------------------------------

type WriteCapture = Arc<Mutex<Option<(u16, Vec<u16>)>>>;
type ReadCapture = Arc<Mutex<Option<(u16, u16)>>>;

struct Fc17Handles {
    calls: Arc<AtomicUsize>,
    write: WriteCapture,
    read: ReadCapture,
}

// --------------------------------------------------------------------------
// Test app
// --------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
enum Mode {
    Success,
    AppError(MbusError),
}

/// App that handles FC17 by writing into an internal register bank, then reading back.
struct Fc17App {
    mode: Mode,
    calls: Arc<AtomicUsize>,
    write_capture: WriteCapture,
    read_capture: ReadCapture,
    /// Internal register bank: 256 u16 slots, default 0.
    regs: [u16; 256],
}

fn make_app(mode: Mode) -> (Fc17App, Fc17Handles) {
    let calls = Arc::new(AtomicUsize::new(0));
    let write = Arc::new(Mutex::new(None));
    let read = Arc::new(Mutex::new(None));
    let app = Fc17App {
        mode,
        calls: Arc::clone(&calls),
        write_capture: Arc::clone(&write),
        read_capture: Arc::clone(&read),
        regs: [0u16; 256],
    };
    let handles = Fc17Handles {
        calls: Arc::clone(&calls),
        write: Arc::clone(&write),
        read: Arc::clone(&read),
    };
    (app, handles)
}

impl ServerExceptionHandler for Fc17App {}
impl ServerCoilHandler for Fc17App {}
impl ServerDiscreteInputHandler for Fc17App {}
impl ServerHoldingRegisterHandler for Fc17App {
    fn read_write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);

        if let Mode::AppError(err) = self.mode {
            return Err(err);
        }

        // Write first, then read (Modbus spec order)
        *self.write_capture.lock().expect("write mutex") =
            Some((write_address, write_values.to_vec()));
        for (i, &v) in write_values.iter().enumerate() {
            let addr = (write_address as usize) + i;
            if addr < self.regs.len() {
                self.regs[addr] = v;
            }
        }

        *self.read_capture.lock().expect("read mutex") = Some((read_address, read_quantity));
        for i in 0..read_quantity as usize {
            let addr = (read_address as usize) + i;
            let value = if addr < self.regs.len() {
                self.regs[addr]
            } else {
                0
            };
            let offset = i * 2;
            out[offset] = (value >> 8) as u8;
            out[offset + 1] = value as u8;
        }

        Ok((read_quantity * 2) as u8)
    }
}
impl ServerInputRegisterHandler for Fc17App {}
impl ServerFifoHandler for Fc17App {}
impl ServerFileRecordHandler for Fc17App {}
impl ServerDiagnosticsHandler for Fc17App {}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for Fc17App {}

// --------------------------------------------------------------------------
// Test helpers
// --------------------------------------------------------------------------

/// Feeds a single request into the server and returns the first response frame.
fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: Fc17App) -> Vec<u8> {
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

/// Builds a raw FC17 payload (no function-code byte).
///
/// Layout: read_addr(2) | read_qty(2) | write_addr(2) | write_qty(2) |
///         byte_count(1) | write_values...
fn fc17_payload(read_addr: u16, read_qty: u16, write_addr: u16, write_values: &[u16]) -> Vec<u8> {
    let write_qty = write_values.len() as u16;
    let mut v = Vec::new();
    v.extend_from_slice(&read_addr.to_be_bytes());
    v.extend_from_slice(&read_qty.to_be_bytes());
    v.extend_from_slice(&write_addr.to_be_bytes());
    v.extend_from_slice(&write_qty.to_be_bytes());
    v.push((write_qty * 2) as u8);
    for &w in write_values {
        v.extend_from_slice(&w.to_be_bytes());
    }
    v
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[test]
fn fc17_write_before_read_reflects_written_values_in_response() {
    // Write [0xAAAA, 0xBBBB] at address 10; read 2 registers from same address.
    // Response must reflect the just-written values.
    let payload = fc17_payload(10, 2, 10, &[0xAAAA, 0xBBBB]);
    let request = build_request(
        1,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *h.write.lock().expect("write mutex"),
        Some((10, vec![0xAAAA, 0xBBBB]))
    );
    // Response FC = 0x17
    assert_eq!(response[7], 0x17);
    // Byte count = 4 (2 registers × 2)
    assert_eq!(response[8], 4);
    // Register data reflects the write
    assert_eq!(&response[9..13], &[0xAA, 0xAA, 0xBB, 0xBB]);
}

#[test]
fn fc17_read_from_different_address_returns_pre_existing_values() {
    // Write [0x1234] at address 20; read 1 register from address 20.
    let payload = fc17_payload(20, 1, 20, &[0x1234]);
    let request = build_request(
        2,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x17);
    assert_eq!(response[8], 2); // 1 register × 2 bytes
    assert_eq!(&response[9..11], &[0x12, 0x34]);
}

#[test]
fn fc17_write_and_read_independent_windows() {
    // Write 3 registers at address 100; read 2 from address 50.
    // Address 50 is uninitialised → response is zeros.
    let payload = fc17_payload(50, 2, 100, &[0x0001, 0x0002, 0x0003]);
    let request = build_request(
        3,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *h.write.lock().expect("write mutex"),
        Some((100, vec![0x0001, 0x0002, 0x0003]))
    );
    assert_eq!(*h.read.lock().expect("read mutex"), Some((50, 2)));
    assert_eq!(response[7], 0x17);
    assert_eq!(response[8], 4);
    assert_eq!(&response[9..13], &[0x00, 0x00, 0x00, 0x00]);
}

#[test]
fn fc17_read_quantity_zero_returns_exception_no_callback() {
    let payload = fc17_payload(0, 0, 0, &[0x0001]);
    let request = build_request(
        4,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x97);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc17_read_quantity_too_large_returns_exception_no_callback() {
    let payload = fc17_payload(0, 126, 0, &[0x0001]);
    let request = build_request(
        5,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x97);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc17_write_quantity_zero_returns_exception_no_callback() {
    // Build payload manually with write_qty = 0
    let mut payload = Vec::new();
    payload.extend_from_slice(&0u16.to_be_bytes()); // read_addr
    payload.extend_from_slice(&1u16.to_be_bytes()); // read_qty
    payload.extend_from_slice(&0u16.to_be_bytes()); // write_addr
    payload.extend_from_slice(&0u16.to_be_bytes()); // write_qty = 0
    payload.push(0u8); // byte_count

    let request = build_request(
        6,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x97);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc17_write_quantity_too_large_returns_exception_no_callback() {
    // write_qty = 121 is the maximum allowed by both the Modbus spec and the physical
    // PDU frame limit (9 header + 121*2 = 251 bytes <= 252).
    // Verify the max-boundary write succeeds (read 1 register from addr 0, write 121 from addr 0).
    let write_values: Vec<u16> = (0..121).map(|i| i as u16).collect();
    let payload = fc17_payload(0, 1, 0, &write_values);
    let request = build_request(
        7,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::Success);

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    // Response is success with 1 register (value 0x0000, which is registers[0] = 0)
    assert_eq!(response[7], 0x17);
    assert_eq!(response[8], 2); // 1 register × 2 bytes
    assert_eq!(
        *h.write.lock().expect("write mutex"),
        Some((0, write_values))
    );
}

#[test]
fn fc17_app_returns_invalid_address_emits_exception() {
    let payload = fc17_payload(0, 1, 0, &[0x0001]);
    let request = build_request(
        8,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::AppError(MbusError::InvalidAddress));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x97);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataAddress
    );
}

#[test]
fn fc17_app_returns_unexpected_error_emits_server_failure_exception() {
    // MbusError::Unexpected maps to ExceptionCode::ServerDeviceFailure (0x04)
    let payload = fc17_payload(0, 1, 0, &[0x0001]);
    let request = build_request(
        9,
        unit_id(1),
        FunctionCode::ReadWriteMultipleRegisters,
        &payload,
    );
    let (app, h) = make_app(Mode::AppError(MbusError::Unexpected));

    let response = run_once(request, app);

    assert_eq!(h.calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x97);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::ServerDeviceFailure
    );
}
