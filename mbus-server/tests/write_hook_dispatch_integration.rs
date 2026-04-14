#![cfg(all(feature = "coils", feature = "holding-registers"))]

mod common;

use common::{MockTransport, build_request, tcp_config, unit_id};
use heapless::Vec as HVec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::{ExceptionCode, MbusError};
use mbus_core::function_codes::public::FunctionCode;
use mbus_server::{
    CoilsModel, HoldingRegistersModel, ResilienceConfig, ServerServices, modbus_app,
};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default, Clone, CoilsModel)]
struct HookCoils {
    #[coil(addr = 0)]
    direct: bool,
    #[coil(addr = 1, notify_via_batch = true)]
    via_batch: bool,
}

#[derive(Debug, Default, Clone, HoldingRegistersModel)]
struct HookRegs {
    #[reg(addr = 10)]
    direct: u16,
    #[reg(addr = 11, notify_via_batch = true)]
    via_batch: u16,
}

#[derive(Debug, Default, Clone)]
#[modbus_app(
    coils(coils, on_batch_write = on_coil_batch, on_write_0 = on_direct_coil),
    holding_registers(regs, on_batch_write = on_reg_batch, on_write_10 = on_direct_reg),
)]
struct DispatchHookApp {
    coils: HookCoils,
    regs: HookRegs,
    reject_coil_batch: bool,
    reject_reg_batch: bool,
    coil_direct_calls: u16,
    reg_direct_calls: u16,
    coil_batch_calls: u16,
    reg_batch_calls: u16,
}

impl DispatchHookApp {
    fn on_direct_coil(&mut self, _address: u16, _old: bool, _new: bool) -> Result<(), MbusError> {
        self.coil_direct_calls += 1;
        Ok(())
    }

    fn on_direct_reg(&mut self, _address: u16, _old: u16, _new: u16) -> Result<(), MbusError> {
        self.reg_direct_calls += 1;
        Ok(())
    }

    fn on_coil_batch(&mut self, _start: u16, _qty: u16, _values: &[u8]) -> Result<(), MbusError> {
        self.coil_batch_calls += 1;
        if self.reject_coil_batch {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
    }

    fn on_reg_batch(&mut self, _start: u16, _qty: u16, _values: &[u16]) -> Result<(), MbusError> {
        self.reg_batch_calls += 1;
        if self.reject_reg_batch {
            return Err(MbusError::InvalidValue);
        }
        Ok(())
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

fn run_once(
    request: HVec<u8, MAX_ADU_FRAME_LEN>,
    app: DispatchHookApp,
) -> (DispatchHookApp, Vec<u8>) {
    let sent_frames = Arc::new(Mutex::new(Vec::new()));

    let transport = MockTransport {
        next_rx: Some(request),
        sent_frames: Arc::clone(&sent_frames),
        connected: true,
    };

    let mut server: ServerServices<MockTransport, DispatchHookApp> = ServerServices::new(
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
        .expect("server should send exactly one response frame");

    (server.app().clone(), response)
}

#[test]
fn fc05_notify_via_batch_rejection_returns_exception_and_preserves_state() {
    let request = build_request(
        61,
        unit_id(1),
        FunctionCode::WriteSingleCoil,
        &[0x00, 0x01, 0xFF, 0x00],
    );

    let app = DispatchHookApp {
        reject_coil_batch: true,
        ..DispatchHookApp::default()
    };

    let (app_after, response) = run_once(request, app);

    assert_eq!(response[7], 0x85);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
    assert_eq!(app_after.coil_batch_calls, 1);
    assert_eq!(app_after.coil_direct_calls, 0);
    assert!(!app_after.coils.via_batch);
}

#[test]
fn fc06_notify_via_batch_success_echoes_and_commits_state() {
    let request = build_request(
        62,
        unit_id(1),
        FunctionCode::WriteSingleRegister,
        &[0x00, 0x0B, 0x12, 0x34],
    );

    let (app_after, response) = run_once(request, DispatchHookApp::default());

    assert_eq!(response[7], 0x06);
    assert_eq!(&response[8..12], &[0x00, 0x0B, 0x12, 0x34]);
    assert_eq!(app_after.reg_batch_calls, 1);
    assert_eq!(app_after.reg_direct_calls, 0);
    assert_eq!(app_after.regs.via_batch, 0x1234);
}

#[test]
fn fc0f_batch_success_commits_values() {
    let request = build_request(
        63,
        unit_id(1),
        FunctionCode::WriteMultipleCoils,
        &[0x00, 0x00, 0x00, 0x02, 0x01, 0x03],
    );

    let (app_after, response) = run_once(request, DispatchHookApp::default());

    assert_eq!(response[7], 0x0F);
    assert_eq!(&response[8..12], &[0x00, 0x00, 0x00, 0x02]);
    assert_eq!(app_after.coil_batch_calls, 1);
    assert!(app_after.coils.direct);
    assert!(app_after.coils.via_batch);
}

#[test]
fn fc10_batch_success_commits_values() {
    let request = build_request(
        64,
        unit_id(1),
        FunctionCode::WriteMultipleRegisters,
        &[0x00, 0x0A, 0x00, 0x02, 0x04, 0x11, 0x11, 0x22, 0x22],
    );

    let (app_after, response) = run_once(request, DispatchHookApp::default());

    assert_eq!(response[7], 0x10);
    assert_eq!(&response[8..12], &[0x00, 0x0A, 0x00, 0x02]);
    assert_eq!(app_after.reg_batch_calls, 1);
    assert_eq!(app_after.regs.direct, 0x1111);
    assert_eq!(app_after.regs.via_batch, 0x2222);
}

#[test]
fn fc10_batch_rejection_returns_exception_and_preserves_state() {
    let request = build_request(
        65,
        unit_id(1),
        FunctionCode::WriteMultipleRegisters,
        &[0x00, 0x0A, 0x00, 0x02, 0x04, 0x11, 0x11, 0x22, 0x22],
    );

    let app = DispatchHookApp {
        reject_reg_batch: true,
        ..DispatchHookApp::default()
    };

    let (app_after, response) = run_once(request, app);

    assert_eq!(response[7], 0x90);
    assert_eq!(
        decode_exception_code(response[8]),
        ExceptionCode::IllegalDataValue
    );
    assert_eq!(app_after.reg_batch_calls, 1);
    assert_eq!(app_after.regs.direct, 0);
    assert_eq!(app_after.regs.via_batch, 0);
}
