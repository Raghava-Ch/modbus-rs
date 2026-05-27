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

use std::net::TcpListener;
use std::time::Duration;
use std::{env, thread::sleep};

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_gateway::{
    DownstreamChannel, GatewayServices, NoopEventHandler, PollOutcome, UnitRouteTable,
};
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, StdRtuTransport, StdTcpServerTransport, Transport,
};

fn main() -> anyhow::Result<()> {
    env_logger::init();

    let bind_addr = env::var("MBUS_GATEWAY_BIND").unwrap_or_else(|_| "127.0.0.1:5502".into());
    let serial_port =
        env::var("MBUS_GATEWAY_SERIAL").unwrap_or_else(|_| "/dev/cu.usbserial-A1010CA6".into());

    // ── Upstream: listen for TCP connections ──────────────────────────────────
    println!("Binding upstream TCP on {bind_addr}");
    let listener = TcpListener::bind(&bind_addr)?;
    println!("Waiting for upstream TCP connection on {bind_addr}");
    let (stream, peer) = listener.accept()?;
    println!("Accepted upstream TCP connection from {peer}");
    let upstream = StdTcpServerTransport::new(stream);

    // ── Downstream: connect to RTU slave ─────────────────────────────────────
    println!("Opening serial downstream on {serial_port}");
    let serial_config = ModbusSerialConfig {
        port_path: serial_port
            .as_str()
            .try_into()
            .map_err(|_| anyhow::anyhow!("serial port path too long"))?,
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Custom(115200),
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 500,
        retry_attempts: 0,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };

    let mut downstream_transport = StdRtuTransport::new();
    downstream_transport.connect(&ModbusConfig::Serial(serial_config))?;
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
    let mut gateway: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _> =
        GatewayServices::new(router, NoopEventHandler, 500);

    gateway.add_upstream(upstream)?;
    gateway.add_downstream(DownstreamChannel::new(downstream_transport))?;

    println!("Gateway running — forwarding TCP → RTU");

    let mut shutdown = false;
    loop {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        if matches!(gateway.poll(now_ms), PollOutcome::AllUpstreamsDisconnected) {
            println!("All upstreams disconnected; shutting down");
            shutdown = true;
        }
        if shutdown {
            break;
        }
        // Yield briefly to avoid hogging CPU in example loop
        sleep(Duration::from_millis(1));
    }
    Ok(())
}
