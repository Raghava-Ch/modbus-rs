use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
use mbus_server::{InputRegistersModel, modbus_app};

#[cfg(feature = "coils")]
use mbus_server::CoilsModel;

#[derive(Debug, Default, InputRegistersModel)]
struct InputsAContiguous {
    #[reg(addr = 0)]
    in_a0: u16,
    #[reg(addr = 1)]
    in_a1: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputsBContiguous {
    #[reg(addr = 2)]
    in_b0: u16,
    #[reg(addr = 3)]
    in_b1: u16,
}

#[derive(Debug, Default)]
#[modbus_app(input_registers(a, b))]
struct InputContiguousApp {
    a: InputsAContiguous,
    b: InputsBContiguous,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputsAGap {
    #[reg(addr = 0)]
    in_a0: u16,
    #[reg(addr = 1)]
    in_a1: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputsBGap {
    #[reg(addr = 3)]
    in_b_only: u16,
}

#[derive(Debug, Default)]
#[modbus_app(input_registers(a, b))]
struct InputGapApp {
    a: InputsAGap,
    b: InputsBGap,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default, CoilsModel)]
struct CoilsAContiguous {
    #[coil(addr = 0)]
    c0: bool,
    #[coil(addr = 1)]
    c1: bool,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default, CoilsModel)]
struct CoilsBContiguous {
    #[coil(addr = 2)]
    c2: bool,
    #[coil(addr = 3)]
    c3: bool,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default)]
#[modbus_app(coils(a, b))]
struct CoilsContiguousApp {
    a: CoilsAContiguous,
    b: CoilsBContiguous,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default, CoilsModel)]
struct CoilsAGap {
    #[coil(addr = 0)]
    c0: bool,
    #[coil(addr = 1)]
    c1: bool,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default, CoilsModel)]
struct CoilsBGap {
    #[coil(addr = 3)]
    c3: bool,
}

#[cfg(feature = "coils")]
#[derive(Debug, Default)]
#[modbus_app(coils(a, b))]
struct CoilsGapApp {
    a: CoilsAGap,
    b: CoilsBGap,
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

#[test]
fn input_registers_segmented_read_can_span_contiguous_maps() {
    let mut app = InputContiguousApp::default();
    app.a.set_in_a0(0x1001);
    app.a.set_in_a1(0x1002);
    app.b.set_in_b0(0x2001);
    app.b.set_in_b1(0x2002);

    let mut out = [0u8; 8];
    let len = app
        .read_multiple_input_registers_request(11, unit_id(1), 0, 4, &mut out)
        .expect("contiguous input-register multi-map read should succeed");

    assert_eq!(len, 8);
    assert_eq!(out, [0x10, 0x01, 0x10, 0x02, 0x20, 0x01, 0x20, 0x02]);
}

#[test]
fn input_registers_segmented_read_across_gap_returns_invalid_address() {
    let mut app = InputGapApp::default();
    app.a.set_in_a0(0x1001);
    app.a.set_in_a1(0x1002);
    app.b.set_in_b_only(0x2002);

    let mut out = [0u8; 8];
    let err = app
        .read_multiple_input_registers_request(12, unit_id(1), 0, 4, &mut out)
        .expect_err("request crossing unmapped input-register gap must fail");

    assert_eq!(err, MbusError::InvalidAddress);
}

#[cfg(feature = "coils")]
#[test]
fn coils_segmented_read_can_span_contiguous_maps() {
    let mut app = CoilsContiguousApp::default();

    app.write_multiple_coils_request(21, unit_id(1), 0, 4, &[0b0000_1101])
        .expect("initialize 4 coils across contiguous maps");

    let mut out = [0u8; 2];
    let len = app
        .read_coils_request(21, unit_id(1), 0, 4, &mut out)
        .expect("contiguous coil multi-map read should succeed");

    assert_eq!(len, 1);
    assert_eq!(out[0], 0b0000_1101);
}

#[cfg(feature = "coils")]
#[test]
fn coils_segmented_write_many_can_span_contiguous_maps() {
    let mut app = CoilsContiguousApp::default();

    // Write 3 coils starting at address 1 with packed bits 0b00000101:
    // addr1=true, addr2=false, addr3=true.
    app.write_multiple_coils_request(22, unit_id(1), 1, 3, &[0b0000_0101])
        .expect("segmented multi-map coil write should succeed");

    let mut out = [0u8; 2];
    let len = app
        .read_coils_request(22, unit_id(1), 0, 4, &mut out)
        .expect("read-back should succeed");

    assert_eq!(len, 1);
    assert_eq!(out[0], 0b0000_1010);
}

#[cfg(feature = "coils")]
#[test]
fn coils_segmented_read_across_gap_returns_invalid_address() {
    let mut app = CoilsGapApp::default();
    app.write_single_coil_request(23, unit_id(1), 0, true)
        .expect("set c0");
    app.write_single_coil_request(23, unit_id(1), 1, false)
        .expect("set c1");
    app.write_single_coil_request(23, unit_id(1), 3, true)
        .expect("set c3");

    let mut out = [0u8; 1];
    let err = app
        .read_coils_request(23, unit_id(1), 0, 4, &mut out)
        .expect_err("request crossing unmapped coil gap must fail");

    assert_eq!(err, MbusError::InvalidAddress);
}
