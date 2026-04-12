use anyhow::Result;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;
use mbus_server::{CoilsModel, HoldingRegistersModel, InputRegistersModel, modbus_app};

#[derive(Debug, Default, HoldingRegistersModel)]
struct HoldingRegs {
    #[reg(addr = 0)]
    setpoint: u16,
    #[reg(addr = 1)]
    mode: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputRegs {
    #[reg(addr = 0)]
    temperature_raw: u16,
    #[reg(addr = 1)]
    pressure_raw: u16,
}

#[derive(Debug, Default, CoilsModel)]
struct CoilBank {
    #[coil(addr = 0)]
    run_enable: bool,
    #[coil(addr = 1)]
    pump_enable: bool,
    #[coil(addr = 2)]
    alarm_ack: bool,
    #[coil(addr = 3)]
    remote_mode: bool,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(holding), input_registers(input), coils(coils))]
struct DemoServer {
    holding: HoldingRegs,
    input: InputRegs,
    coils: CoilBank,
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

// NOTE — Ownership model options
//
// This example calls the generated `ModbusAppHandler` methods directly on the app struct.
// This is the simplest approach and works well when a single owner drives the server loop
// (e.g., a bare-metal or single-threaded embedded task).
//
// When you need shared ownership — for example to update process values from one thread
// while another thread services requests — use `ForwardingApp<A>` instead:
//
//   let app = Arc::new(Mutex::new(DemoServer::default()));
//   let fwd = ForwardingApp::new(Arc::clone(&app));
//   // pass `fwd` to ServerServices; update `app` from your process loop
//
// See `std_transport_client_demo.rs` for a full working example of that pattern,
// or the "Ownership models" section in modbus-rs/README.md.

fn main() -> Result<()> {
    let mut app = DemoServer::default();
    let unit = unit_id(1);

    // Seed read-only input registers (FC04) from internal process values.
    app.input.set_temperature_raw(245);
    app.input.set_pressure_raw(1013);

    // Holding register services: FC06 + FC10 write, FC03 read.
    app.write_single_register_request(100, unit, 0, 900)?;
    app.write_multiple_registers_request(101, unit, 1, &[2])?;

    let mut hold_out = [0u8; 4];
    let hold_len = app.read_multiple_holding_registers_request(102, unit, 0, 2, &mut hold_out)?;
    assert_eq!(hold_len, 4);
    assert_eq!(hold_out, [0x03, 0x84, 0x00, 0x02]);

    // Input register service: FC04 read.
    let mut in_out = [0u8; 4];
    let in_len = app.read_multiple_input_registers_request(103, unit, 0, 2, &mut in_out)?;
    assert_eq!(in_len, 4);
    assert_eq!(in_out, [0x00, 0xF5, 0x03, 0xF5]);

    // Coil services: FC05 + FC0F write, FC01 read.
    app.write_single_coil_request(104, unit, 0, true)?;
    app.write_multiple_coils_request(105, unit, 1, 3, &[0b0000_0101])?;

    let mut coil_out = [0u8; 2];
    let coil_len = app.read_coils_request(106, unit, 0, 4, &mut coil_out)?;
    assert_eq!(coil_len, 1);
    assert_eq!(coil_out[0], 0b0000_1011);

    println!("Server showcase completed.");
    println!("Holding registers [0..2): {:?}", hold_out);
    println!("Input registers [0..2):   {:?}", in_out);
    println!("Coils [0..4):             {:08b}", coil_out[0]);

    // Future service groups can be added incrementally by extending `#[modbus_app(...)]`
    // and adding matching derived map structs.

    Ok(())
}
