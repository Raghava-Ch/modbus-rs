//! # WebSocket → RTU serial downstream gateway
//!
//! Bridges browser WASM clients (over WebSocket) to Modbus RTU slaves connected
//! to a local RS-485/RS-232 serial bus.
//!
//! ```text
//! Browser (WASM)                  Gateway (this example)          RTU Bus
//! ───────────────                 ─────────────────────────       ───────────────
//! WasmModbusClient                AsyncWsGatewayServer            Slave 1
//!   ↕ WebSocket ────────────►      TokioRtuTransport  ──────►     Slave 2
//!   (MBAP framing)                  (RTU framing on /dev/ttyUSB0)  …
//! ```
//!
//! The gateway translates from Modbus TCP ADU framing (used by `WasmModbusClient`)
//! to Modbus RTU framing (binary + CRC16) transparently.  No changes to either
//! the browser client or the RTU device are required.
//!
//! ## Hardware setup
//!
//! - RS-485 USB adapter plugged into `/dev/ttyUSB0` (Linux) or `COM3` (Windows).
//! - RTU slaves at addresses 1–8 on the bus, wired to the adapter.
//!
//! ## Run
//!
//! ```text
//! cargo run --example ws_to_rtu \
//!     --features ws-server,serial-rtu \
//!     -p mbus-gateway
//! ```
//!
//! Note: requires the `serial-rtu` feature in addition to `ws-server` because
//! the Tokio async serial transport lives in `mbus-serial`.

// ── Implementation (requires ws-server + serial-rtu features) ─────────────────

#[cfg(all(feature = "ws-server", feature = "serial-rtu"))]
fn main() {
    // Import here so the feature gate is respected at the item level.
    use std::sync::Arc;
    use std::time::Duration;

    use mbus_core::transport::{
        BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
        Parity, SerialMode, UnitIdOrSlaveAddr,
    };
    use mbus_gateway::{AsyncWsGatewayServer, UnitRouteTable, WsGatewayConfig};
    // TokioRtuTransport is re-exported from mbus_serial when serial-rtu is active.
    use mbus_serial::TokioRtuTransport;
    use tokio::sync::Mutex;

    /// Serial port path — adjust for your OS and hardware.
    const SERIAL_PORT: &str = if cfg!(target_os = "windows") {
        "COM2"
    } else {
        "/dev/ttyUSB0"
    };

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // ── Downstream RTU transport ──────────────────────────────────────
            let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig {
                port_path: SERIAL_PORT
                    .try_into()
                    .expect("serial port path too long"),
                mode: SerialMode::Rtu,
                baud_rate: BaudRate::Baud19200,
                data_bits: DataBits::Eight,
                stop_bits: 1,
                parity: Parity::None,
                response_timeout_ms: 1000,
                retry_attempts: 0,
                retry_backoff_strategy: BackoffStrategy::Immediate,
                retry_jitter_strategy: JitterStrategy::None,
                retry_random_fn: None,
            });
            let rtu = TokioRtuTransport::new(&serial_cfg).expect("open serial port");
            let shared_rtu = Arc::new(Mutex::new(rtu));

            // ── Route units 1–8 to channel 0 (the single RTU bus) ────────────
            let mut router: UnitRouteTable<8> = UnitRouteTable::new();
            for unit in 1u8..=8 {
                router
                    .add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0)
                    .unwrap();
            }

            // ── Config ────────────────────────────────────────────────────────
            let config = WsGatewayConfig {
                // RTU round-trips are slow; allow generous idle window.
                idle_timeout: Some(Duration::from_secs(60)),
                // Single serial bus serialises traffic; cap concurrent clients.
                max_sessions: 8,
                require_modbus_subprotocol: false,
                allowed_origins: Vec::new(),
            };

            println!("WebSocket gateway on ws://0.0.0.0:8502");
            println!("Downstream RTU bus: {} @ 19200 baud", SERIAL_PORT);

            AsyncWsGatewayServer::serve("0.0.0.0:8502", config, router, vec![shared_rtu])
                .await
                .expect("gateway error");
        });
}

#[cfg(not(all(feature = "ws-server", feature = "serial-rtu")))]
fn main() {
    eprintln!(
        "This example requires both the `ws-server` and `serial-rtu` features.\n\
         Re-run with:\n\
         \n    cargo run --example ws_to_rtu --features ws-server,serial-rtu -p mbus-gateway"
    );
}
