use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerHoldingRegisterHandler;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{modbus_app, CoilsModel, HoldingRegistersModel};

#[derive(Default, CoilsModel)]
struct HvacCoils {
    #[coil(addr = 0)]
    compressor_online: bool,
    #[coil(addr = 1)]
    alarm_active: bool,
    #[coil(addr = 2)]
    maintenance_required: bool,
}

#[derive(Default, HoldingRegistersModel)]
struct HvacHolding {
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    current_temp: u16,
    #[reg(addr = 1, scale = 0.1, unit = "C")]
    setpoint_temp: u16,
    #[reg(addr = 2)]
    runtime_hours: u16,
}

#[derive(Default)]
#[modbus_app(holding_registers(holding), coils(coils))]
struct HvacApp {
    holding: HvacHolding,
    coils: HvacCoils,
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for HvacApp {}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

#[test]
fn fc03_routes_and_returns_scaled_words() {
    let mut app = HvacApp::default();

    app.holding
        .set_current_temp_scaled(21.5)
        .expect("scaled write should fit");
    app.holding.set_setpoint_temp(220);
    app.holding.set_runtime_hours(1234);

    let mut out = [0u8; 6];
    let len = app
        .read_multiple_holding_registers_request(1, unit_id(1), 0, 3, &mut out)
        .expect("FC03 route should succeed");

    assert_eq!(len, 6);
    assert_eq!(out, [0x00, 0xD7, 0x00, 0xDC, 0x04, 0xD2]);
}

#[test]
fn fc06_routes_and_updates_model() {
    let mut app = HvacApp::default();

    app.write_single_register_request(2, unit_id(1), 1, 245)
        .expect("FC06 route should succeed");

    assert!((app.holding.setpoint_temp_scaled() - 24.5).abs() < 0.01);
}

#[test]
fn fc01_fc05_route_through_coil_map() {
    let mut app = HvacApp::default();

    app.write_single_coil_request(3, unit_id(1), 0, true)
        .expect("FC05 route should succeed");
    app.write_single_coil_request(4, unit_id(1), 1, true)
        .expect("FC05 route should succeed");

    let mut out = [0u8; 1];
    let len = app
        .read_coils_request(5, unit_id(1), 0, 3, &mut out)
        .expect("FC01 route should succeed");

    assert_eq!(len, 1);
    assert_eq!(out[0], 0b0000_0011);
}

#[test]
fn request_gap_returns_invalid_address() {
    #[derive(Default, HoldingRegistersModel)]
    #[reg(allow_gaps)]
    struct GapRegs {
        #[reg(addr = 0)]
        a: u16,
        #[reg(addr = 2)]
        b: u16,
    }

    #[derive(Default)]
    #[modbus_app(holding_registers(regs))]
    struct GapApp {
        regs: GapRegs,
    }

    #[cfg(feature = "traffic")]
    impl mbus_server::TrafficNotifier for GapApp {}

    let mut app = GapApp::default();
    app.regs.set_a(1);
    app.regs.set_b(2);

    let mut out = [0u8; 6];
    let err = app
        .read_multiple_holding_registers_request(6, unit_id(1), 0, 3, &mut out)
        .expect_err("range crossing an unmapped address must fail");

    assert_eq!(err, MbusError::InvalidAddress);
}
