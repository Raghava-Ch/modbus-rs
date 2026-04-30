#![cfg(feature = "holding-registers")]

use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerHoldingRegisterHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{HoldingRegistersModel, modbus_app};

#[derive(Debug, Default, HoldingRegistersModel)]
struct ChillerContiguous {
    #[reg(addr = 0)]
    temp_a: u16,
    #[reg(addr = 1)]
    temp_b: u16,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct CompressorContiguous {
    #[reg(addr = 2)]
    pressure_a: u16,
    #[reg(addr = 3)]
    pressure_b: u16,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(chiller, compressor))]
struct ContiguousApp {
    chiller: ChillerContiguous,
    compressor: CompressorContiguous,
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for ContiguousApp {}

#[derive(Debug, Default, HoldingRegistersModel)]
struct ChillerWithGap {
    #[reg(addr = 0)]
    temp_a: u16,
    #[reg(addr = 1)]
    temp_b: u16,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct CompressorWithGap {
    #[reg(addr = 3)]
    pressure_only: u16,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(chiller, compressor))]
struct GapApp {
    chiller: ChillerWithGap,
    compressor: CompressorWithGap,
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for GapApp {}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

#[test]
fn fc03_can_span_multiple_contiguous_maps() {
    let mut app = ContiguousApp::default();
    app.chiller.set_temp_a(0x1111);
    app.chiller.set_temp_b(0x2222);
    app.compressor.set_pressure_a(0x3333);
    app.compressor.set_pressure_b(0x4444);

    let mut out = [0u8; 8];
    let len = app
        .read_multiple_holding_registers_request(1, unit_id(1), 0, 4, &mut out)
        .expect("contiguous multi-map read should succeed");

    assert_eq!(len, 8);
    assert_eq!(out, [0x11, 0x11, 0x22, 0x22, 0x33, 0x33, 0x44, 0x44]);
}

#[test]
fn fc03_span_across_gap_returns_invalid_address() {
    let mut app = GapApp::default();
    app.chiller.set_temp_a(0x1111);
    app.chiller.set_temp_b(0x2222);
    app.compressor.set_pressure_only(0x4444);

    let mut out = [0u8; 8];
    let err = app
        .read_multiple_holding_registers_request(7, unit_id(1), 0, 4, &mut out)
        .expect_err("request crossing an unmapped gap must fail");

    assert_eq!(err, MbusError::InvalidAddress);
}
