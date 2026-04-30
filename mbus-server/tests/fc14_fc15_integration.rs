#![cfg(feature = "file-record")]

mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::models::file_record::FILE_RECORD_REF_TYPE;
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

type ReadCalls = Arc<Mutex<Vec<(u16, u16, u16)>>>;
type WriteCalls = Arc<Mutex<Vec<(u16, u16, Vec<u16>)>>>;

struct Handles {
    read_count: Arc<AtomicUsize>,
    write_count: Arc<AtomicUsize>,
    reads: ReadCalls,
    writes: WriteCalls,
}

#[derive(Debug, Clone, Copy)]
enum Mode {
    Ok,
    ReadErr(MbusError),
    WriteErr(MbusError),
}

struct FileRecordApp {
    mode: Mode,
    read_count: Arc<AtomicUsize>,
    write_count: Arc<AtomicUsize>,
    reads: ReadCalls,
    writes: WriteCalls,
    files: [[u16; 128]; 4],
}

fn make_app(mode: Mode) -> (FileRecordApp, Handles) {
    let read_count = Arc::new(AtomicUsize::new(0));
    let write_count = Arc::new(AtomicUsize::new(0));
    let reads = Arc::new(Mutex::new(Vec::new()));
    let writes = Arc::new(Mutex::new(Vec::new()));

    let mut files = [[0u16; 128]; 4];
    for (file_idx, file) in files.iter_mut().enumerate() {
        for (index, slot) in file.iter_mut().enumerate() {
            *slot = ((file_idx as u16 + 1) << 12) | index as u16;
        }
    }

    let app = FileRecordApp {
        mode,
        read_count: Arc::clone(&read_count),
        write_count: Arc::clone(&write_count),
        reads: Arc::clone(&reads),
        writes: Arc::clone(&writes),
        files,
    };

    let handles = Handles {
        read_count,
        write_count,
        reads,
        writes,
    };
    (app, handles)
}

impl ServerExceptionHandler for FileRecordApp {}
impl ServerCoilHandler for FileRecordApp {}
impl ServerDiscreteInputHandler for FileRecordApp {}
impl ServerHoldingRegisterHandler for FileRecordApp {}
impl ServerInputRegisterHandler for FileRecordApp {}
impl ServerFifoHandler for FileRecordApp {}
impl ServerFileRecordHandler for FileRecordApp {
    fn read_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.read_count.fetch_add(1, Ordering::SeqCst);
        self.reads
            .lock()
            .expect("reads mutex")
            .push((file_number, record_number, record_length));

        if let Mode::ReadErr(err) = self.mode {
            return Err(err);
        }

        if !(1..=4).contains(&file_number) {
            return Err(MbusError::InvalidAddress);
        }
        let start = record_number as usize;
        let len = record_length as usize;
        if start.checked_add(len).is_none() || start + len > 128 {
            return Err(MbusError::InvalidAddress);
        }

        let file_idx = (file_number - 1) as usize;
        for i in 0..len {
            let value = self.files[file_idx][start + i];
            out[i * 2] = (value >> 8) as u8;
            out[i * 2 + 1] = value as u8;
        }

        Ok((record_length * 2) as u8)
    }

    fn write_file_record_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: &[u16],
    ) -> Result<(), MbusError> {
        self.write_count.fetch_add(1, Ordering::SeqCst);
        self.writes.lock().expect("writes mutex").push((
            file_number,
            record_number,
            record_data.to_vec(),
        ));

        if let Mode::WriteErr(err) = self.mode {
            return Err(err);
        }

        if record_data.len() != record_length as usize {
            return Err(MbusError::InvalidByteCount);
        }
        if !(1..=4).contains(&file_number) {
            return Err(MbusError::InvalidAddress);
        }
        let start = record_number as usize;
        let len = record_length as usize;
        if start.checked_add(len).is_none() || start + len > 128 {
            return Err(MbusError::InvalidAddress);
        }

        let file_idx = (file_number - 1) as usize;
        for (i, &value) in record_data.iter().enumerate() {
            self.files[file_idx][start + i] = value;
        }

        Ok(())
    }
}
impl ServerDiagnosticsHandler for FileRecordApp {}

#[cfg(feature = "traffic")]
impl mbus_server::TrafficNotifier for FileRecordApp {}

fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: FileRecordApp) -> Vec<u8> {
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
        _ => panic!("unexpected exception: {value:#04x}"),
    }
}

fn fc14_payload(sub_requests: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0);
    for &(file_number, record_number, record_length) in sub_requests {
        out.push(FILE_RECORD_REF_TYPE);
        out.extend_from_slice(&file_number.to_be_bytes());
        out.extend_from_slice(&record_number.to_be_bytes());
        out.extend_from_slice(&record_length.to_be_bytes());
    }
    out[0] = (out.len() - 1) as u8;
    out
}

fn fc15_payload(sub_requests: &[(u16, u16, &[u16])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(0);
    for &(file_number, record_number, record_data) in sub_requests {
        out.push(FILE_RECORD_REF_TYPE);
        out.extend_from_slice(&file_number.to_be_bytes());
        out.extend_from_slice(&record_number.to_be_bytes());
        out.extend_from_slice(&(record_data.len() as u16).to_be_bytes());
        for &value in record_data {
            out.extend_from_slice(&value.to_be_bytes());
        }
    }
    out[0] = (out.len() - 1) as u8;
    out
}

#[test]
fn fc14_single_sub_request_success() {
    let payload = fc14_payload(&[(2, 3, 2)]);
    let request = build_request(1, unit_id(1), FunctionCode::ReadFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x14);
    assert_eq!(response[8], 6); // [sub_len, ref, data(4)]
    assert_eq!(response[9], 5); // 1 (ref) + 4 data bytes
    assert_eq!(response[10], FILE_RECORD_REF_TYPE);
    assert_eq!(&response[11..13], &[0x20, 0x03]);
    assert_eq!(&response[13..15], &[0x20, 0x04]);
    assert_eq!(h.read_count.load(Ordering::SeqCst), 1);
    assert_eq!(*h.reads.lock().expect("reads"), vec![(2, 3, 2)]);
}

#[test]
fn fc14_multiple_sub_requests_success() {
    let payload = fc14_payload(&[(1, 0, 1), (3, 2, 2)]);
    let request = build_request(2, unit_id(1), FunctionCode::ReadFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x14);
    assert_eq!(response[8], 10);
    // sub 1
    assert_eq!(response[9], 3);
    assert_eq!(response[10], FILE_RECORD_REF_TYPE);
    assert_eq!(&response[11..13], &[0x10, 0x00]);
    // sub 2
    assert_eq!(response[13], 5);
    assert_eq!(response[14], FILE_RECORD_REF_TYPE);
    assert_eq!(&response[15..17], &[0x30, 0x02]);
    assert_eq!(&response[17..19], &[0x30, 0x03]);
    assert_eq!(h.read_count.load(Ordering::SeqCst), 2);
}

#[test]
fn fc14_invalid_reference_type_emits_exception_without_callback() {
    let payload = vec![7, 0x05, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01];
    let request = build_request(3, unit_id(1), FunctionCode::ReadFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x94);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
    assert_eq!(h.read_count.load(Ordering::SeqCst), 0);
}

#[test]
fn fc14_app_error_maps_to_illegal_data_address() {
    let payload = fc14_payload(&[(1, 0, 1)]);
    let request = build_request(4, unit_id(1), FunctionCode::ReadFileRecord, &payload);
    let (app, h) = make_app(Mode::ReadErr(MbusError::InvalidAddress));

    let response = run_once(request, app);

    assert_eq!(response[7], 0x94);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataAddress
    );
    assert_eq!(h.read_count.load(Ordering::SeqCst), 1);
}

#[test]
fn fc14_record_number_overflow_returns_exception_without_callback() {
    // record_number + (record_length - 1) overflows u16 and should be rejected
    // before the app callback is invoked.
    let payload = fc14_payload(&[(1, 0xFFFF, 2)]);
    let request = build_request(41, unit_id(1), FunctionCode::ReadFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x94);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataAddress
    );
    assert_eq!(h.read_count.load(Ordering::SeqCst), 0);
}

#[test]
fn fc15_single_sub_request_success_echoes_request() {
    let payload = fc15_payload(&[(1, 5, &[0xAAAA, 0xBBBB])]);
    let request = build_request(5, unit_id(1), FunctionCode::WriteFileRecord, &payload);
    let request_vec = request.as_slice().to_vec();
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response, request_vec);
    assert_eq!(h.write_count.load(Ordering::SeqCst), 1);
    assert_eq!(
        *h.writes.lock().expect("writes"),
        vec![(1, 5, vec![0xAAAA, 0xBBBB])]
    );
}

#[test]
fn fc15_multiple_sub_requests_success_echoes_request() {
    let payload = fc15_payload(&[(2, 1, &[0x1111]), (4, 3, &[0x2222, 0x3333])]);
    let request = build_request(6, unit_id(1), FunctionCode::WriteFileRecord, &payload);
    let request_vec = request.as_slice().to_vec();
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response, request_vec);
    assert_eq!(h.write_count.load(Ordering::SeqCst), 2);
}

#[test]
fn fc15_record_length_data_mismatch_returns_exception_no_callback() {
    // byte_count=9, but record_length says 2 regs while only 1 reg (2 bytes) is present.
    let payload = vec![
        9,
        FILE_RECORD_REF_TYPE,
        0x00,
        0x01,
        0x00,
        0x00,
        0x00,
        0x02,
        0xAA,
        0xBB,
    ];
    let request = build_request(7, unit_id(1), FunctionCode::WriteFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x95);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataValue
    );
    assert_eq!(h.write_count.load(Ordering::SeqCst), 0);
}

#[test]
fn fc15_unexpected_app_error_maps_to_server_device_failure() {
    let payload = fc15_payload(&[(1, 0, &[0x5555])]);
    let request = build_request(8, unit_id(1), FunctionCode::WriteFileRecord, &payload);
    let (app, h) = make_app(Mode::WriteErr(MbusError::Unexpected));

    let response = run_once(request, app);

    assert_eq!(response[7], 0x95);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::ServerDeviceFailure
    );
    assert_eq!(h.write_count.load(Ordering::SeqCst), 1);
}

#[test]
fn fc15_record_number_overflow_returns_exception_without_callback() {
    // record_number + (record_length - 1) overflows u16 and should be rejected
    // before the app callback is invoked.
    let payload = fc15_payload(&[(1, 0xFFFF, &[0x1111, 0x2222])]);
    let request = build_request(42, unit_id(1), FunctionCode::WriteFileRecord, &payload);
    let (app, h) = make_app(Mode::Ok);

    let response = run_once(request, app);

    assert_eq!(response[7], 0x95);
    assert_eq!(
        decode_exception(response[8]),
        ExceptionCode::IllegalDataAddress
    );
    assert_eq!(h.write_count.load(Ordering::SeqCst), 0);
}
