//! # TCP → RTU gateway example
//!
//! Demonstrates a synchronous Modbus gateway that:
//! - Accepts **one** upstream TCP connection on port 5020
//! - Bridges it to a downstream RTU device on a serial port
//! - Routes unit 1–10 → channel 0 (the single RTU bus)
//!
//! ## Build & run
//!
//! ```bash
//! cargo run --example tcp_to_rtu \
//!     --features network,serial-rtu \
//!     -p mbus-gateway
//! ```
//!
//! The serial port path (`/dev/ttyUSB0` on Linux, `COM3` on Windows) and baud
//! rate are hard-coded below; adjust them for your hardware.

fn main() {
    use std::net::TcpListener;

    use mbus_core::transport::{
        BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
        Parity, SerialMode, Transport,
    };
    use mbus_gateway::{
        DownstreamChannel, GatewayServices, NoopEventHandler, PollOutcome, RangeRouteTable,
        StdRtuTransport, StdTcpServerTransport,
    };

    // ── Configuration ─────────────────────────────────────────────────────────
    const LISTEN_ADDR: &str = "0.0.0.0:5020";
    const SERIAL_PORT: &str = if cfg!(target_os = "windows") {
        "COM2"
    } else {
        "/dev/ttys005"
    };

    // ── Routing table: unit IDs 1–10 all go to downstream channel 0 ──────────
    let mut router: RangeRouteTable<4> = RangeRouteTable::new();
    router.add(1, 10, 0).expect("range 1–10 → channel 0");

    // ── Upstream: accept a single TCP connection ───────────────────────────────
    let listener = TcpListener::bind(LISTEN_ADDR).expect("bind failed");
    eprintln!("[gateway] listening on {LISTEN_ADDR} …");

    let (stream, peer) = listener.accept().expect("accept failed");
    eprintln!("[gateway] accepted upstream connection from {peer}");

    let upstream = StdTcpServerTransport::new(stream);

    // ── Downstream: open the RTU serial port ──────────────────────────────────
    let mut downstream = StdRtuTransport::new();
    let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: SERIAL_PORT.try_into().expect("serial port path too long"),
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
    downstream
        .connect(&serial_cfg)
        .expect("serial port open failed");
    eprintln!("[gateway] downstream RTU port {SERIAL_PORT} opened");

    // ── Gateway ───────────────────────────────────────────────────────────────
    let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _> =
        GatewayServices::new(router, NoopEventHandler, 1000);
    gw.add_upstream(upstream)
        .expect("upstream channel registered");
    gw.add_downstream(DownstreamChannel::new(downstream))
        .expect("channel slot available");

    eprintln!("[gateway] running — press Ctrl-C to stop");
    loop {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        match gw.poll(now_ms) {
            PollOutcome::AllUpstreamsDisconnected => {
                eprintln!("[gateway] all upstreams disconnected");
                break;
            }
            _ => {}
        }
        // Yield briefly to avoid hogging 100% CPU in this example loop
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
