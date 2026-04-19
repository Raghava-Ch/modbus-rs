//! Async Modbus TCP server example.
//!
//! Demonstrates zero-boilerplate `#[async_modbus_app]`:
//! ```
//! #[async_modbus_app(holding_registers(holding), coils(coils))]
//! struct HvacApp { ... }
//! AsyncTcpServer::serve("0.0.0.0:502", HvacApp::default(), unit_id(1)).await?;
//! ```
//!
//! Run with:
//! ```text
//! cargo run --example async_server_tcp \
//!   --features server-tcp,coils,registers
//! ```
//!
//! Test it with a Modbus TCP client, for example `mbpoll`:
//! ```text
//! mbpoll -m tcp -a 1 -t 4 -r 1 -c 3 127.0.0.1
//! ```

use anyhow::Result;
use mbus_async::server::{AsyncTcpServer};
use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::{CoilsModel, HoldingRegistersModel, async_modbus_app};
use std::sync::Arc;
use tokio::sync::Mutex;

// ── data models ──────────────────────────────────────────────────────────────

/// A simple HVAC controller coil bank.
#[derive(Default, CoilsModel)]
struct HvacCoils {
    #[coil(addr = 0)]
    compressor_online: bool,
    #[coil(addr = 1)]
    alarm_active: bool,
    #[coil(addr = 2)]
    maintenance_required: bool,
}

/// HVAC holding registers.
#[derive(Default, HoldingRegistersModel)]
struct HvacHolding {
    /// Current temperature × 10 (e.g. 215 → 21.5 °C).
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    current_temp: u16,
    /// Setpoint × 10.
    #[reg(addr = 1, scale = 0.1, unit = "C")]
    setpoint_temp: u16,
    /// Cumulative runtime in hours.
    #[reg(addr = 2)]
    runtime_hours: u16,
}

// ── Level 1 application — async_modbus_app macro ─────────────────────────────

/// Full HVAC application struct.
///
/// The `#[async_modbus_app]` macro generates:
/// - All sync `ModbusAppHandler` split-trait impls (for backwards compatibility)
/// - An `AsyncAppHandler` impl with an `async fn handle()` dispatcher
///
/// Async write hooks (optional) are declared inline and receive `.await` calls
/// in the generated code.
#[derive(Default)]
#[async_modbus_app(holding_registers(holding, on_write_1 = on_setpoint_temp_write), coils(coils))]
struct HvacApp {
    holding: HvacHolding,
    coils: HvacCoils,
}

/// Hook called after every single-register write — could log, persist, notify, …
impl HvacApp {
    async fn on_setpoint_temp_write(
        &mut self,
        addr: u16,
        _old: u16,
        new: u16,
    ) -> Result<(), MbusError> {
        println!("  [hook] register {addr:#06x} updated → {new:#06x}");
        Ok(())
    }
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncTrafficNotifier for HvacApp {}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

// ── main ─────────────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> Result<()> {
    run_level1().await
}

// ── Level 1: #[async_modbus_app] ─────────────────────────────────────────────

async fn run_level1() -> Result<()> {
    println!("Starting Level 1 async Modbus TCP server on 127.0.0.1:5502 …");
    println!("Press Ctrl-C to stop.\n");

    let mut app = HvacApp::default();
    // Pre-seed some values so the server returns interesting data
    app.holding.set_current_temp(215); // 21.5 °C
    app.holding.set_setpoint_temp(220); // 22.0 °C
    app.holding.set_runtime_hours(42);
    // CoilsModel generates CoilMap trait impl, not individual setters — use write_single
    use mbus_server::CoilMap as _;
    app.coils.write_single(0, true).ok(); // compressor_online = true

    // Wrap in Arc<Mutex<_>> so multiple concurrent sessions share one app instance.
    let shared = Arc::new(Mutex::new(app));

    // This call runs forever; errors from individual sessions are silently dropped.
    let _: std::convert::Infallible =
        AsyncTcpServer::serve_shared("127.0.0.1:5502", shared, unit_id(1))
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
    unreachable!()
}
