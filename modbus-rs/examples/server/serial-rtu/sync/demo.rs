use anyhow::{Context, Result};
use mbus_server::{
    CoilsModel, HoldingRegistersModel, InputRegistersModel, ResilienceConfig, ServerServices,
    modbus_app,
};
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, StdRtuTransport, UnitIdOrSlaveAddr,
};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

#[derive(Debug, Default, HoldingRegistersModel)]
struct HoldingRegs {
    #[reg(addr = 0)]
    setpoint_tenths_c: u16,
    #[reg(addr = 1)]
    mode: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct InputRegs {
    #[reg(addr = 0)]
    temperature_tenths_c: u16,
    #[reg(addr = 1)]
    pressure_kpa: u16,
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

fn baud_from_u32(v: u32) -> BaudRate {
    match v {
        9600 => BaudRate::Baud9600,
        19200 => BaudRate::Baud19200,
        custom => BaudRate::Custom(custom),
    }
}

fn parse_cli() -> Result<(String, u8, u32)> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("Usage: server_serial_rtu_demo [--port PATH] [--unit UNIT] [--baud BAUD]");
        std::process::exit(0);
    }

    let mut port = std::env::var("MBUS_SERVER_SERIAL_PORT").unwrap_or_else(|_| {
        if cfg!(windows) {
            "COM3".to_string()
        } else {
            "/dev/ttyUSB0".to_string()
        }
    });
    let mut unit = std::env::var("MBUS_SERVER_UNIT")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);
    let mut baud = std::env::var("MBUS_SERVER_BAUD")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(19200);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].clone();
                i += 2;
            }
            "--unit" if i + 1 < args.len() => {
                unit = args[i + 1].parse::<u8>().context("invalid --unit")?;
                i += 2;
            }
            "--baud" if i + 1 < args.len() => {
                baud = args[i + 1].parse::<u32>().context("invalid --baud")?;
                i += 2;
            }
            other => {
                return Err(anyhow::anyhow!("unknown argument `{other}`"));
            }
        }
    }

    Ok((port, unit, baud))
}

fn seed_app() -> DemoServer {
    let mut app = DemoServer::default();

    app.holding.set_setpoint_tenths_c(220);
    app.holding.set_mode(2);

    app.input.set_temperature_tenths_c(245);
    app.input.set_pressure_kpa(1013);

    app.coils.run_enable = true;
    app.coils.pump_enable = true;
    app.coils.alarm_ack = false;
    app.coils.remote_mode = true;

    app
}

fn main() -> Result<()> {
    let (port_path, unit_raw, baud_raw) = parse_cli()?;

    let config = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(&port_path)
            .map_err(|_| anyhow::anyhow!("serial port path too long"))?,
        mode: SerialMode::Rtu,
        baud_rate: baud_from_u32(baud_raw),
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 100,
        retry_attempts: 1,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    });

    let transport = StdRtuTransport::new();
    let app = seed_app();
    let resilience = ResilienceConfig {
        enable_broadcast_writes: true,
        ..ResilienceConfig::default()
    };

    let mut server = ServerServices::new(transport, app, config, unit_id(unit_raw), resilience);
    server.connect()?;

    println!("Modbus RTU server running on {}", port_path);
    println!("Unit id: {}", unit_raw);
    println!("Baud: {}", baud_raw);
    println!("Supported now: FC01, FC03, FC04, FC05, FC06, FC0F, FC10");

    loop {
        server.poll();
        thread::sleep(Duration::from_millis(1));
    }
}
