//! Sync TCP-to-RTU gateway example.
//!
//! Accepts Modbus TCP requests on port 5502 and forwards them to a Modbus RTU
//! slave on a serial port.  The gateway is poll-driven — no async runtime
//! required.
//!
//! # Usage
//!
//! ```text
//! MBUS_GATEWAY_BIND=0.0.0.0:5502 \
//! MBUS_GATEWAY_SERIAL=/dev/ttyUSB0 \
//!   cargo run --example modbus_rs_gateway_sync_tcp_to_rtu \
//!     --features gateway,serial-rtu,network-tcp
//! ```

use std::env;
use std::time::Duration;

use modbus_rs::{
    BaudRate, DataBits, ModbusSerialConfig, ModbusTcpConfig, Parity, SerialMode, StdRtuTransport,
    StdTcpServerTransport, StdTcpTransport,
};
use mbus_gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bind_addr = env::var("MBUS_GATEWAY_BIND").unwrap_or_else(|_| "127.0.0.1:5502".into());
    let serial_port = env::var("MBUS_GATEWAY_SERIAL").unwrap_or_else(|_| "/dev/ttyUSB0".into());

    // ── Upstream: listen for TCP connections ──────────────────────────────────
    println!("Binding upstream TCP on {bind_addr}");
    let tcp_config = ModbusTcpConfig {
        host: bind_addr.split(':').next().unwrap_or("127.0.0.1").to_string(),
        port: bind_addr.split(':').nth(1).and_then(|p| p.parse().ok()).unwrap_or(5502),
        response_timeout_ms: 1000,
        connection_timeout_ms: 5000,
    };

    let mut upstream = StdTcpServerTransport::new();
    upstream.connect(&modbus_rs::ModbusConfig::Tcp(tcp_config))?;
    println!("Upstream TCP transport ready");

    // ── Downstream: connect to RTU slave ─────────────────────────────────────
    println!("Opening serial downstream on {serial_port}");
    let serial_config = ModbusSerialConfig {
        port: serial_port.clone(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: modbus_rs::transport::StopBits::One,
        response_timeout_ms: 500,
        mode: SerialMode::Rtu,
    };

    let mut downstream_transport = StdRtuTransport::new();
    downstream_transport.connect(&modbus_rs::ModbusConfig::Serial(serial_config))?;
    println!("Serial downstream ready");

    // ── Routing table ─────────────────────────────────────────────────────────
    // Route all units 1–32 to channel 0 (the single RTU bus).
    let mut router: UnitRouteTable<32> = UnitRouteTable::new();
    for unit_id in 1u8..=32 {
        if let Ok(uid) = UnitIdOrSlaveAddr::new(unit_id) {
            router.add(uid, 0).ok();
        }
    }

    // ── Gateway ───────────────────────────────────────────────────────────────
    let mut gateway: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _, 1> =
        GatewayServices::new(upstream, router, NoopEventHandler);

    gateway.add_downstream(DownstreamChannel::new(downstream_transport))?;
    // For RTU serial transports, one recv call blocks up to the read timeout.
    gateway.set_max_downstream_recv_attempts(1);

    println!("Gateway running — forwarding TCP → RTU");

    loop {
        match gateway.poll() {
            Ok(()) => {}
            Err(mbus_core::errors::MbusError::Timeout) => {
                // Normal: no data available or downstream timed out.
            }
            Err(e) => {
                eprintln!("Gateway error: {e:?}");
            }
        }
    }
}
