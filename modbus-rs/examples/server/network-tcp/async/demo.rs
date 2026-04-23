//! Async Modbus TCP server — HVAC demo.
//!
//! Demonstrates a shared-state async server using `#[async_modbus_app]` and
//! `AsyncTcpServer::serve_shared`.  All accepted connections share a single
//! `Arc<tokio::sync::Mutex<HvacServerApp>>`.  A background task simulates a
//! slowly-drifting zone temperature so clients can observe live register changes.
//!
//! Run:
//! ```text
//! cargo run --example modbus_rs_server_async_tcp_demo \
//!     --features "server,async,network-tcp,coils,holding-registers,input-registers"
//! ```
//!
//! Then connect any Modbus TCP client to 127.0.0.1:5502 (unit 1).

use anyhow::{Context, Result};
use mbus_async::server::AsyncTcpServer;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{CoilsModel, HoldingRegistersModel, InputRegistersModel, async_modbus_app};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

// ── Data models ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, HoldingRegistersModel)]
struct HvacHoldingRegs {
    /// Zone setpoint in tenths of a degree C (e.g. 220 = 22.0 °C).
    #[reg(addr = 0)]
    zone_setpoint_tenths_c: u16,
    /// Fan mode: 0 = off, 1 = on, 2 = auto.
    #[reg(addr = 1)]
    fan_mode: u16,
}

#[derive(Debug, Default, InputRegistersModel)]
struct HvacInputRegs {
    /// Live zone temperature in tenths of a degree C.
    #[reg(addr = 0)]
    zone_temp_tenths_c: u16,
    /// Discharge air temperature in tenths of a degree C.
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

// ── Application struct ───────────────────────────────────────────────────────

/// Main server application.
///
/// The `#[async_modbus_app]` macro generates an `AsyncAppHandler` impl that
/// dispatches each supported function code to the matching named field.
#[derive(Debug, Default)]
#[async_modbus_app(holding_registers(holding), input_registers(input), coils(coils))]
struct HvacServerApp {
    holding: HvacHoldingRegs,
    input: HvacInputRegs,
    coils: HvacCoils,
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncTrafficNotifier for HvacServerApp {}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn parse_cli() -> Result<(String, u16, u8)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: modbus_rs_server_async_tcp_demo [--host HOST] [--port PORT] [--unit UNIT]"
        );
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

fn seed_app() -> HvacServerApp {
    let mut app = HvacServerApp::default();
    app.holding.set_zone_setpoint_tenths_c(220); // 22.0 °C
    app.holding.set_fan_mode(2); // auto
    app.input.set_zone_temp_tenths_c(245); // 24.5 °C
    app.input.set_discharge_temp_tenths_c(301); // 30.1 °C
    app.coils.fan_enable = true;
    app.coils.remote_override = true;
    app
}

/// Spawns a background task that slowly oscillates the simulated zone temperature.
fn spawn_telemetry_task(shared: Arc<Mutex<HvacServerApp>>) {
    tokio::spawn(async move {
        let mut temp: u16 = 245;
        let mut up = true;
        loop {
            {
                let mut app = shared.lock().await;
                app.input.set_zone_temp_tenths_c(temp);
                app.input
                    .set_discharge_temp_tenths_c(temp.saturating_add(55));
            }
            if up {
                temp = (temp + 1).min(265);
                if temp >= 265 {
                    up = false;
                }
            } else {
                temp = temp.saturating_sub(1).max(225);
                if temp <= 225 {
                    up = true;
                }
            }
            sleep(Duration::from_millis(500)).await;
        }
    });
}

// ── Entry point ──────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    let (host, port, unit_raw) = parse_cli()?;
    let bind = format!("{host}:{port}");
    let unit = unit_id(unit_raw);

    println!(
        "Async HVAC Modbus TCP server on {bind}  (unit {})",
        unit.get()
    );
    println!("Supported FC: 01, 03, 04, 05, 06, 0F, 10");
    println!("Holding  addr 0 = setpoint (tenths °C), addr 1 = fan_mode");
    println!("Input    addr 0 = zone temp,            addr 1 = discharge temp");

    let shared = Arc::new(Mutex::new(seed_app()));
    spawn_telemetry_task(shared.clone());

    // AsyncTcpServer::serve_shared runs the accept loop forever, handing each
    // connection to its own tokio task while all tasks share the same app mutex.
    AsyncTcpServer::serve_shared(&bind, shared, unit)
        .await
        .context("server error")?;

    Ok(())
}
