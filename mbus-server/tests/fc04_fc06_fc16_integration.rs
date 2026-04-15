mod common;
use common::{MockTransport, build_request, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
use mbus_server::ResilienceConfig;
use mbus_server::ServerServices;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

type Fc06Last = Arc<Mutex<Option<(u16, u16)>>>;
type Fc16Last = Arc<Mutex<Option<(u16, Vec<u16>)>>>;
type MaskFc16Last = Arc<Mutex<Option<(u16, u16, u16)>>>;

#[derive(Debug, Clone, Copy)]
enum RegisterMode {
    Success,
    AppError(MbusError),
}

#[derive(Debug)]
struct RegisterApp {
    mode_fc04: RegisterMode,
    mode_fc06: RegisterMode,
    mode_fc16: RegisterMode,
    mode_mask_fc16: RegisterMode,
    fc04_calls: Arc<AtomicUsize>,
    fc06_calls: Arc<AtomicUsize>,
    fc16_calls: Arc<AtomicUsize>,
    mask_fc16_calls: Arc<AtomicUsize>,
    fc06_last: Fc06Last,
    fc16_last: Fc16Last,
    mask_fc16_last: MaskFc16Last,
}

impl ModbusAppHandler for RegisterApp {
    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.fc04_calls.fetch_add(1, Ordering::SeqCst);
        match self.mode_fc04 {
            RegisterMode::Success => {
                for i in 0..quantity as usize {
                    let value = address.wrapping_add(i as u16);
                    let offset = i * 2;
                    out[offset] = (value >> 8) as u8;
                    out[offset + 1] = value as u8;
                }
                Ok((quantity * 2) as u8)
            }
            RegisterMode::AppError(error) => Err(error),
        }
    }

    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        _out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        self.fc06_calls.fetch_add(1, Ordering::SeqCst);
        *self.fc06_last.lock().expect("fc06 mutex poisoned") = Some((address, value));

        match self.mode_fc06 {
            RegisterMode::Success => Ok(()),
            RegisterMode::AppError(error) => Err(error),
        }
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        self.fc16_calls.fetch_add(1, Ordering::SeqCst);
        *self.fc16_last.lock().expect("fc16 mutex poisoned") =
            Some((starting_address, values.to_vec()));

        match self.mode_fc16 {
            RegisterMode::Success => Ok(()),
            RegisterMode::AppError(error) => Err(error),
        }
    }

    fn mask_write_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        self.mask_fc16_calls.fetch_add(1, Ordering::SeqCst);
        *self
            .mask_fc16_last
            .lock()
            .expect("mask_fc16 mutex poisoned") = Some((address, and_mask, or_mask));

        match self.mode_mask_fc16 {
            RegisterMode::Success => Ok(()),
            RegisterMode::AppError(error) => Err(error),
        }
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for RegisterApp {}

fn run_once(request: HVec<u8, MAX_ADU_FRAME_LEN>, app: RegisterApp) -> (RegisterApp, Vec<u8>) {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: sent_frames.clone(),
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

    let response = sent_frames
        .lock()
        .expect("sent_frames mutex poisoned")
        .first()
        .cloned()
        .expect("server should send exactly one frame");

    (server.app().to_owned_for_test(), response)
}

trait CloneForTest {
    fn to_owned_for_test(&self) -> RegisterApp;
}

impl CloneForTest for RegisterApp {
    fn to_owned_for_test(&self) -> RegisterApp {
        RegisterApp {
            mode_fc04: self.mode_fc04,
            mode_fc06: self.mode_fc06,
            mode_fc16: self.mode_fc16,
            mode_mask_fc16: self.mode_mask_fc16,
            fc04_calls: Arc::clone(&self.fc04_calls),
            fc06_calls: Arc::clone(&self.fc06_calls),
            fc16_calls: Arc::clone(&self.fc16_calls),
            mask_fc16_calls: Arc::clone(&self.mask_fc16_calls),
            fc06_last: Arc::clone(&self.fc06_last),
            fc16_last: Arc::clone(&self.fc16_last),
            mask_fc16_last: Arc::clone(&self.mask_fc16_last),
        }
    }
}

fn decode_exception_code(value: u8) -> ExceptionCode {
    match value {
        0x01 => ExceptionCode::IllegalFunction,
        0x02 => ExceptionCode::IllegalDataAddress,
        0x03 => ExceptionCode::IllegalDataValue,
        0x04 => ExceptionCode::ServerDeviceFailure,
        _ => panic!("unexpected exception code: {value:#04x}"),
    }
}

fn make_app(
    mode_fc04: RegisterMode,
    mode_fc06: RegisterMode,
    mode_fc16: RegisterMode,
    mode_mask_fc16: RegisterMode,
) -> RegisterApp {
    RegisterApp {
        mode_fc04,
        mode_fc06,
        mode_fc16,
        mode_mask_fc16,
        fc04_calls: Arc::new(AtomicUsize::new(0)),
        fc06_calls: Arc::new(AtomicUsize::new(0)),
        fc16_calls: Arc::new(AtomicUsize::new(0)),
        mask_fc16_calls: Arc::new(AtomicUsize::new(0)),
        fc06_last: Arc::new(Mutex::new(None)),
        fc16_last: Arc::new(Mutex::new(None)),
        mask_fc16_last: Arc::new(Mutex::new(None)),
    }
}

#[test]
fn fc04_success_returns_register_payload() {
    let address = 0x0010;
    let quantity = 3u16;
    let request = build_request(
        7,
        unit_id(1),
        FunctionCode::ReadInputRegisters,
        &[0x00, 0x10, 0x00, 0x03],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::AppError(MbusError::InvalidFunctionCode),
        RegisterMode::AppError(MbusError::InvalidFunctionCode),
        RegisterMode::AppError(MbusError::InvalidFunctionCode),
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc04_calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x04);
    assert_eq!(response[8], 6);
    assert_eq!(&response[9..15], &[0x00, 0x10, 0x00, 0x11, 0x00, 0x12]);
    let _ = (address, quantity);
}

#[test]
fn fc04_invalid_quantity_returns_exception_before_app_callback() {
    let request = build_request(
        8,
        unit_id(1),
        FunctionCode::ReadInputRegisters,
        &[0x00, 0x01, 0x00, 0x00],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc04_calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x84);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc06_success_echoes_address_and_value() {
    let request = build_request(
        9,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x2A, 0x12, 0x34],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc06_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *app.fc06_last.lock().expect("fc06 mutex poisoned"),
        Some((0x002A, 0x1234))
    );
    assert_eq!(response[7], 0x06);
    assert_eq!(&response[8..12], &[0x00, 0x2A, 0x12, 0x34]);
}

#[test]
fn fc06_app_error_returns_exception() {
    let request = build_request(
        10,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x2A, 0x00, 0x01],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::AppError(MbusError::InvalidAddress),
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc06_calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[7], 0x86);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataAddress
    );
}

#[test]
fn fc16_success_writes_values_and_echoes_window() {
    let request = build_request(
        11,
        unit_id(1),
        FunctionCode::WriteMultipleRegisters,
        &[0x00, 0x30, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x00, 0x0B],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc16_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *app.fc16_last.lock().expect("fc16 mutex poisoned"),
        Some((0x0030, vec![0x000A, 0x000B]))
    );
    assert_eq!(response[7], 0x10);
    assert_eq!(&response[8..12], &[0x00, 0x30, 0x00, 0x02]);
}

#[test]
fn fc16_quantity_zero_returns_exception_before_app_callback() {
    let request = build_request(
        12,
        unit_id(1),
        FunctionCode::WriteMultipleRegisters,
        &[0x00, 0x30, 0x00, 0x00, 0x00],
    );
    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app, response) = run_once(request, app);

    assert_eq!(app.fc16_calls.load(Ordering::SeqCst), 0);
    assert_eq!(response[7], 0x90);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
}

#[test]
fn fc16_mask_write_success_echoes_request_fields() {
    let request = build_request(
        17,
        unit_id(1),
        FunctionCode::MaskWriteRegister,
        &[0x00, 0x10, 0xFF, 0x00, 0x00, 0x0F],
    );

    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
    );

    let (app_after, response) = run_once(request, app);

    assert_eq!(app_after.fc04_calls.load(Ordering::SeqCst), 0);
    assert_eq!(app_after.fc06_calls.load(Ordering::SeqCst), 0);
    assert_eq!(app_after.fc16_calls.load(Ordering::SeqCst), 0);
    assert_eq!(app_after.mask_fc16_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        *app_after
            .mask_fc16_last
            .lock()
            .expect("mask_fc16 mutex poisoned"),
        Some((0x0010, 0xFF00, 0x000F))
    );

    assert_eq!(response[0..2], [0x00, 0x11]);
    assert_eq!(response[7], 0x16);
    assert_eq!(&response[8..14], &[0x00, 0x10, 0xFF, 0x00, 0x00, 0x0F]);
}

#[test]
fn fc16_mask_write_app_error_returns_exception() {
    let request = build_request(
        18,
        unit_id(1),
        FunctionCode::MaskWriteRegister,
        &[0x00, 0x10, 0xFF, 0x00, 0x00, 0x0F],
    );

    let app = make_app(
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::Success,
        RegisterMode::AppError(MbusError::InvalidAddress),
    );

    let (app_after, response) = run_once(request, app);

    assert_eq!(app_after.mask_fc16_calls.load(Ordering::SeqCst), 1);
    assert_eq!(response[0..2], [0x00, 0x12]);
    assert_eq!(response[7], 0x96);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataAddress
    );
}
