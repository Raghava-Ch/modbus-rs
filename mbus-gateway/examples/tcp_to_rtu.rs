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

#[cfg(all(feature = "network", feature = "serial-rtu"))]
fn main() {
    use std::net::TcpListener;

    use mbus_core::transport::{BaudRate, ModbusConfig, Parity, SerialConfig, UnitIdOrSlaveAddr};
    use mbus_gateway::{
        DownstreamChannel, GatewayServices, NoopEventHandler, RangeRouteTable, StdRtuTransport,
        StdTcpServerTransport,
    };

    // ── Configuration ─────────────────────────────────────────────────────────
    const LISTEN_ADDR: &str = "0.0.0.0:5020";
    const SERIAL_PORT: &str = if cfg!(target_os = "windows") {
        "COM3"
    } else {
        "/dev/ttyUSB0"
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
    let serial_cfg = ModbusConfig::Serial(
        SerialConfig::builder()
            .port(SERIAL_PORT)
            .baud_rate(BaudRate::Baud19200)
            .parity(Parity::None)
            .build()
            .expect("valid serial config"),
    );
    downstream
        .connect(&serial_cfg)
        .expect("serial port open failed");
    eprintln!("[gateway] downstream RTU port {SERIAL_PORT} opened");

    // ── Gateway ───────────────────────────────────────────────────────────────
    let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _, 1> =
        GatewayServices::new(upstream, router, NoopEventHandler);
    // Serial is blocking with its own timeout; one recv attempt is enough.
    gw.set_max_downstream_recv_attempts(1);
    gw.add_downstream(DownstreamChannel::new(downstream))
        .expect("channel slot available");

    eprintln!("[gateway] running — press Ctrl-C to stop");
    loop {
        match gw.poll() {
            Ok(()) => {}
            Err(e) => {
                eprintln!("[gateway] poll error: {e:?}");
                break;
            }
        }
    }
}

#[cfg(not(all(feature = "network", feature = "serial-rtu")))]
fn main() {
    eprintln!(
        "This example requires the `network` and `serial-rtu` features.\n\
         Re-run with: cargo run --example tcp_to_rtu \
         --features network,serial-rtu -p mbus-gateway"
    );
}
