mod common;
use common::{MockTransport, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::{compile_adu_frame, MAX_ADU_FRAME_LEN, Pdu};
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};
use mbus_server::ServerServices;
use mbus_server::ModbusAppHandler;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
struct GuardedApp {
    calls: Arc<AtomicUsize>,
    response_len: u8,
}

impl GuardedApp {
    fn new(calls: Arc<AtomicUsize>, response_len: u8) -> Self {
        Self {
            calls,
            response_len,
        }
    }
}

impl ModbusAppHandler for GuardedApp {
    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        _out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if !out.is_empty() {
            out[0] = 0x12;
            if out.len() > 1 {
                out[1] = 0x34;
            }
        }
        Ok(self.response_len)
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        _value: u16,
    ) -> Result<(), MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _starting_address: u16,
        _values: &[u16],
    ) -> Result<(), MbusError> {
        Err(MbusError::InvalidFunctionCode)
    }
}

fn build_fc03_request(
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    address: u16,
    quantity: u16,
) -> HVec<u8, MAX_ADU_FRAME_LEN> {
    let request_data = [
        (address >> 8) as u8,
        address as u8,
        (quantity >> 8) as u8,
        quantity as u8,
    ];
    let pdu = Pdu::new(
        FunctionCode::ReadHoldingRegisters,
        HVec::from_slice(&request_data).expect("request data should fit in PDU"),
        request_data.len() as u8,
    );

    compile_adu_frame(txn_id, unit.get(), pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}

fn run_single_request(
    address: u16,
    quantity: u16,
    app_response_len: u8,
) -> (usize, u8, ExceptionCode) {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));
    let app_calls = Arc::new(AtomicUsize::new(0));

    let transport = MockTransport {
        next_rx: Some(build_fc03_request(1, unit_id(1), address, quantity)),
        sent_frames: sent_frames.clone(),
        connected: true,
    };
    let app = GuardedApp::new(app_calls.clone(), app_response_len);

    let mut server = ServerServices::new(transport, app, tcp_config(), unit_id(1));
    server.poll();

    let frames = sent_frames.lock().expect("sent_frames mutex poisoned");
    assert_eq!(
        frames.len(),
        1,
        "server should send exactly one response frame"
    );

    assert!(
        frames[0].len() >= 9,
        "TCP exception response must contain MBAP(7) + FC(1) + EX(1)"
    );
    let fc = frames[0][7];
    let ex = decode_exception_code(frames[0][8]).expect("valid exception code");

    (app_calls.load(Ordering::SeqCst), fc, ex)
}

fn decode_exception_code(value: u8) -> Result<ExceptionCode, MbusError> {
    match value {
        0x01 => Ok(ExceptionCode::IllegalFunction),
        0x02 => Ok(ExceptionCode::IllegalDataAddress),
        0x03 => Ok(ExceptionCode::IllegalDataValue),
        0x04 => Ok(ExceptionCode::ServerDeviceFailure),
        _ => Err(MbusError::InvalidByteCount),
    }
}

#[test]
fn fc03_quantity_zero_rejected_before_app_callback() {
    let (calls, fc, ex) = run_single_request(0, 0, 2);

    assert_eq!(
        calls, 0,
        "invalid quantity must be rejected before app callback"
    );
    assert_eq!(fc, 0x83);
    assert_eq!(ex, ExceptionCode::IllegalDataValue);
}

#[test]
fn fc03_quantity_above_125_rejected_before_app_callback() {
    let (calls, fc, ex) = run_single_request(0, 126, 2);

    assert_eq!(
        calls, 0,
        "quantity > 125 must be rejected before app callback"
    );
    assert_eq!(fc, 0x83);
    assert_eq!(ex, ExceptionCode::IllegalDataValue);
}

#[test]
fn fc03_address_range_overflow_rejected_before_app_callback() {
    let (calls, fc, ex) = run_single_request(0xFFFF, 2, 4);

    assert_eq!(
        calls, 0,
        "overflowing address range must be rejected before app callback"
    );
    assert_eq!(fc, 0x83);
    assert_eq!(ex, ExceptionCode::IllegalDataAddress);
}

#[test]
fn fc03_byte_count_mismatch_returns_exception_response() {
    // quantity=2 -> expected byte count is 4, app deliberately returns 2
    let (calls, fc, ex) = run_single_request(0, 2, 2);

    assert_eq!(
        calls, 1,
        "app should be called for structurally valid request"
    );
    assert_eq!(fc, 0x83);
    assert_eq!(ex, ExceptionCode::IllegalDataValue);
}

#[test]
fn fc03_oversized_app_byte_count_no_panic_and_returns_exception() {
    // quantity=1 -> expected byte count is 2, app returns 255 (oversized)
    let (calls, fc, ex) = run_single_request(0, 1, 255);

    assert_eq!(
        calls, 1,
        "server should handle oversized app return without panic"
    );
    assert_eq!(fc, 0x83);
    assert_eq!(ex, ExceptionCode::IllegalDataValue);
}
