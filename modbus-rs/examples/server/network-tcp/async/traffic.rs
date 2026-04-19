//! Async Modbus TCP server — traffic-logging demo.
//!
//! Demonstrates how to implement [`AsyncTrafficNotifier`] to intercept every raw
//! ADU frame that flows through the server session: received frames, sent frames,
//! framing errors, transmit errors and the separate `on_exception` callback that
//! fires whenever the server generates a Modbus exception response.
//!
//! The application itself is a minimal in-memory coil + holding-register store
//! decorated with `#[async_modbus_app]`.
//!
//! Run:
//! ```text
//! cargo run --example modbus_rs_server_async_tcp_traffic \
//!     --features "server,async-server-tcp,coils,holding-registers,traffic"
//! ```
//!
//! Then poke it with any Modbus client, e.g.:
//! ```text
//! mbpoll -m tcp -a 1 -t 0 -r 1 -c 4 127.0.0.1 -p 5502
//! ```

use anyhow::{Context, Result};
use mbus_async::server::{AsyncTcpServer, AsyncTrafficDirection, AsyncTrafficNotifier};
use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{CoilsModel, HoldingRegistersModel, async_modbus_app};
use std::sync::Arc;
use tokio::sync::Mutex;

// ── Data models ─────────────────────────────────────────────────────────────

#[derive(Debug, Default, CoilsModel)]
struct AppCoils {
    #[coil(addr = 0)]
    relay_0: bool,
    #[coil(addr = 1)]
    relay_1: bool,
    #[coil(addr = 2)]
    relay_2: bool,
    #[coil(addr = 3)]
    relay_3: bool,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct AppRegs {
    #[reg(addr = 0)]
    control: u16,
    #[reg(addr = 1)]
    setpoint: u16,
    #[reg(addr = 2)]
    mode: u16,
}

// ── Application ──────────────────────────────────────────────────────────────

#[derive(Debug, Default)]
#[async_modbus_app(coils(coils), holding_registers(regs))]
struct TrafficApp {
    coils: AppCoils,
    regs: AppRegs,
}

/// Traffic hook implementation.
///
/// [`AsyncTrafficNotifier`] is only required when the `traffic` feature is
/// enabled.  All four methods have default no-ops — override only the ones you
/// need.
#[cfg(feature = "traffic")]
impl AsyncTrafficNotifier for TrafficApp {
    /// Fires for every successfully received and parsed ADU *before* dispatch.
    fn on_rx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        log_frame(AsyncTrafficDirection::Rx, txn_id, unit, frame, None);
    }

    /// Fires after each response ADU is flushed to the transport.
    fn on_tx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        log_frame(AsyncTrafficDirection::Tx, txn_id, unit, frame, None);
    }

    /// Fires when the server cannot send a response (e.g. broken pipe mid-write).
    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        log_frame(AsyncTrafficDirection::Tx, txn_id, unit, frame, Some(error));
    }

    /// Fires when an incoming frame cannot be parsed (CRC failure, truncated
    /// ADU, etc.).
    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        log_frame(AsyncTrafficDirection::Rx, txn_id, unit, frame, Some(error));
    }
}

// ── Shared logging helper ────────────────────────────────────────────────────

/// Formats a single ADU frame to stdout, tagged by direction and optional error.
///
/// `AsyncTrafficDirection` lets callers log Rx/Tx uniformly without branching.
#[cfg(feature = "traffic")]
fn log_frame(
    dir: AsyncTrafficDirection,
    txn_id: u16,
    unit: UnitIdOrSlaveAddr,
    frame: &[u8],
    error: Option<MbusError>,
) {
    let tag = match dir {
        AsyncTrafficDirection::Rx => "RX",
        AsyncTrafficDirection::Tx => "TX",
    };
    let err_suffix = match &error {
        Some(e) => format!(" ERROR={e}"),
        None => String::new(),
    };
    print!(
        "[{tag}] txn={txn_id:04x} unit={:3} len={}{err_suffix}  ",
        unit.get(),
        frame.len(),
    );
    for b in frame {
        print!("{b:02X} ");
    }
    println!();
}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn parse_cli() -> Result<(String, u16, u8)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        println!(
            "Usage: modbus_rs_server_async_tcp_traffic \
            [--host HOST] [--port PORT] [--unit UNIT]"
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
            other => return Err(anyhow::anyhow!("unknown argument `{other}`")),
        }
    }

    Ok((host, port, unit))
}

#[tokio::main]
async fn main() -> Result<()> {
    let (host, port, unit_raw) = parse_cli()?;
    let bind = format!("{host}:{port}");

    println!("Async Modbus TCP server (traffic logging) on {bind}  unit {unit_raw}");
    println!("Every raw ADU frame will be logged below.");
    println!();

    let shared = Arc::new(Mutex::new(TrafficApp::default()));

    // AsyncTcpServer::serve_shared drives the accept loop.  Each session shares
    // the same Arc<Mutex<TrafficApp>> and therefore the same traffic log sink.
    AsyncTcpServer::serve_shared(&bind, shared, unit_id(unit_raw))
        .await
        .context("server exited unexpectedly")?;

    Ok(())
}
