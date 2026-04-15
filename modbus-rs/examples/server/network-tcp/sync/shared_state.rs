use anyhow::{Context, Result};
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, UnitIdOrSlaveAddr};
use mbus_network::AcceptedTcpTransport;
use mbus_server::{
    CoilsModel, ForwardingApp, HoldingRegistersModel, InputRegistersModel, ModbusAppAccess,
    ResilienceConfig, ServerServices, modbus_app,
};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

#[derive(Debug, Default, HoldingRegistersModel)]
struct HvacHoldingRegs {
    #[reg(addr = 0)]
    zone_setpoint_tenths_c: u16,
    #[reg(addr = 1)]
    fan_mode: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct HvacInputRegs {
    #[reg(addr = 0)]
    zone_temp_tenths_c: u16,
    #[reg(addr = 1)]
    discharge_temp_tenths_c: u16,
}

#[derive(Debug, Default, CoilsModel)]
struct HvacCoils {
    #[coil(addr = 0)]
    compressor_enable: bool,
    #[coil(addr = 1)]
    fan_enable: bool,
    #[coil(addr = 2)]
    alarm_ack: bool,
    #[coil(addr = 3)]
    remote_override: bool,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(holding), input_registers(input), coils(coils))]
struct HvacServerApp {
    holding: HvacHoldingRegs,
    input: HvacInputRegs,
    coils: HvacCoils,
}

#[derive(Clone)]
struct SharedHvacApp {
    // Runtime-owned shared state. The protocol stack does not depend on this
    // concrete lock type; access is exposed through ModbusAppAccess below.
    inner: Arc<Mutex<HvacServerApp>>,
}

impl SharedHvacApp {
    fn new(seed: HvacServerApp) -> Self {
        Self {
            inner: Arc::new(Mutex::new(seed)),
        }
    }

    fn with_mut<R>(&self, f: impl FnOnce(&mut HvacServerApp) -> R) -> R {
        let mut guard = self.inner.lock().expect("shared hvac state lock poisoned");
        f(&mut guard)
    }
}

impl ModbusAppAccess for SharedHvacApp {
    type App = HvacServerApp;

    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R,
    {
        // Single delegation point used by ForwardingApp for all FC callbacks.
        self.with_mut(f)
    }
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn parse_cli() -> Result<(String, u16, u8)> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("Usage: std_transport_client_demo [--host HOST] [--port PORT] [--unit UNIT]");
        std::process::exit(0);
    }

    let mut host = std::env::var("MBUS_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let mut port = std::env::var("MBUS_SERVER_PORT")
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .unwrap_or(5502);
    let mut unit = std::env::var("MBUS_SERVER_UNIT")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--host" if i + 1 < args.len() => {
                host = args[i + 1].clone();
                i += 2;
            }
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].parse::<u16>().context("invalid --port")?;
                i += 2;
            }
            "--unit" if i + 1 < args.len() => {
                unit = args[i + 1].parse::<u8>().context("invalid --unit")?;
                i += 2;
            }
            other => {
                return Err(anyhow::anyhow!("unknown argument `{other}`"));
            }
        }
    }

    Ok((host, port, unit))
}

fn seed_server_app() -> HvacServerApp {
    let mut app = HvacServerApp::default();
    app.holding.set_zone_setpoint_tenths_c(220); // 22.0C
    app.holding.set_fan_mode(2); // auto
    app.input.set_zone_temp_tenths_c(245); // 24.5C
    app.input.set_discharge_temp_tenths_c(301); // 30.1C
    app.coils.compressor_enable = false;
    app.coils.fan_enable = true;
    app.coils.alarm_ack = false;
    app.coils.remote_override = true;
    app
}

fn spawn_telemetry_updater(shared: SharedHvacApp) {
    thread::spawn(move || {
        let mut temp = 245u16;
        let mut direction_up = true;

        loop {
            shared.with_mut(|app| {
                app.input.set_zone_temp_tenths_c(temp);
                app.input
                    .set_discharge_temp_tenths_c(temp.saturating_add(55));

                if direction_up {
                    temp = (temp + 1).min(265);
                    if temp >= 265 {
                        direction_up = false;
                    }
                } else {
                    temp = temp.saturating_sub(1).max(225);
                    if temp <= 225 {
                        direction_up = true;
                    }
                }
            });

            thread::sleep(Duration::from_millis(500));
        }
    });
}

fn run_server_loop(host: &str, port: u16, unit: UnitIdOrSlaveAddr) -> Result<()> {
    let bind = format!("{host}:{port}");
    let listener = TcpListener::bind(&bind).with_context(|| format!("failed to bind {bind}"))?;

    let shared = SharedHvacApp::new(seed_server_app());
    spawn_telemetry_updater(shared.clone());

    println!("HVAC Modbus server running on {bind}");
    println!("Unit id: {}", unit.get());
    println!("Supported FC: 01, 03, 04, 05, 06, 0F, 10");
    println!(
        "Model: addr0=setpoint(0.1C), addr1=fan_mode, input0=zone temp, input1=discharge temp"
    );

    for incoming in listener.incoming() {
        match incoming {
            Ok(stream) => {
                let peer = stream
                    .peer_addr()
                    .map(|v| v.to_string())
                    .unwrap_or_else(|_| "unknown".to_string());

                println!("client connected: {peer}");

                let mut cfg = ModbusTcpConfig::new(host, port).expect("server tcp config");
                cfg.response_timeout_ms = 100;
                // ForwardingApp removes per-callback delegation boilerplate.
                let app = ForwardingApp::new(shared.clone());
                thread::spawn(move || {
                    let transport = AcceptedTcpTransport::new(stream);
                    let mut server = ServerServices::new(
                        transport,
                        app,
                        ModbusConfig::Tcp(cfg),
                        unit,
                        ResilienceConfig::default(),
                    );

                    if let Err(err) = server.connect() {
                        eprintln!("server connect failed for {peer}: {err}");
                        return;
                    }

                    while server.is_connected() {
                        server.poll();
                        thread::sleep(Duration::from_millis(1));
                    }

                    println!("client disconnected: {peer}");
                });
            }
            Err(err) => {
                eprintln!("listener error: {err}");
                thread::sleep(Duration::from_millis(50));
            }
        }
    }

    Ok(())
}

fn main() -> Result<()> {
    let (host, port, unit_raw) = parse_cli()?;
    run_server_loop(&host, port, unit_id(unit_raw))
}
