//! # RTU serial upstream → TCP downstream async gateway
//!
//! Bridges an RS-485/RS-232 Modbus RTU **master** (physical PLC, SCADA box,
//! or any serial Modbus master) to TCP-connected Modbus TCP slave devices.
//!
//! ```text
//! RTU Master (PLC)                 Gateway (this example)        TCP Slaves
//! ─────────────────                ──────────────────────────    ──────────────────
//! Modbus RTU master                AsyncSerialGatewayServer      192.168.1.10:502
//!   ↕ RS-485 on /dev/ttyUSB0  ───► TokioTcpTransport (ch 0) ──► 192.168.1.11:502
//!   (RTU framing)                    (TCP framing to downstream)
//! ```
//!
//! The gateway receives RTU-framed Modbus requests from the serial master,
//! translates the ADU to Modbus TCP framing, forwards to the downstream TCP
//! slave, and returns the TCP response re-encoded as RTU.
//!
//! ## Run
//!
//! ```text
//! cargo run --example rtu_upstream_to_tcp \
//!     --features serial-rtu-async \
//!     -p mbus-gateway
//! ```

#[cfg(feature = "serial-rtu-async")]
fn main() {
    use std::sync::Arc;

    use mbus_core::transport::{
        BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
        Parity, SerialMode, UnitIdOrSlaveAddr,
    };
    use mbus_gateway::{
        AsyncSerialGatewayServer, GatewayShutdown, NoopEventHandler, TokioRtuTransport,
        UnitRouteTable,
    };
    use mbus_network::TokioTcpTransport;
    use tokio::sync::Mutex;

    const SERIAL_PORT: &str = if cfg!(target_os = "windows") {
        "COM2"
    } else {
        "/dev/ttyUSB0"
    };

    const DOWNSTREAM_ADDR: &str = "192.168.1.10:502";

    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            // ── Open the RTU upstream serial port ────────────────────────────
            let serial_cfg = ModbusConfig::Serial(ModbusSerialConfig {
                port_path: SERIAL_PORT.try_into().expect("port path too long"),
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
            let rtu_upstream =
                TokioRtuTransport::new(&serial_cfg).expect("failed to open serial port");

            // ── Connect to the downstream TCP slave ───────────────────────────
            let downstream = TokioTcpTransport::connect(DOWNSTREAM_ADDR)
                .await
                .expect("failed to connect to downstream");
            let shared_ds = Arc::new(Mutex::new(downstream));

            // ── Route units 1–8 to channel 0 ─────────────────────────────────
            let mut router: UnitRouteTable<8> = UnitRouteTable::new();
            for unit in 1u8..=8 {
                router
                    .add(UnitIdOrSlaveAddr::new(unit).unwrap(), 0)
                    .unwrap();
            }

            // ── Graceful shutdown on Ctrl+C ───────────────────────────────────
            let (token, shutdown) = GatewayShutdown::new();
            tokio::spawn(async move {
                tokio::signal::ctrl_c()
                    .await
                    .expect("ctrl-c handler failed");
                println!("shutdown signal received");
                token.cancel();
            });

            println!(
                "Serial upstream gateway: {} @ 19200 → {}",
                SERIAL_PORT, DOWNSTREAM_ADDR
            );

            let handler = Arc::new(Mutex::new(NoopEventHandler));
            AsyncSerialGatewayServer::serve_with_shutdown(
                rtu_upstream,
                router,
                vec![shared_ds],
                handler,
                std::time::Duration::from_secs(1),
                shutdown,
            )
            .await
            .expect("gateway error");
        });
}

#[cfg(not(feature = "serial-rtu-async"))]
fn main() {
    eprintln!(
        "This example requires the `serial-rtu-async` feature.\n\
         Re-run with:\n\
         \n    cargo run --example rtu_upstream_to_tcp --features serial-rtu-async -p mbus-gateway"
    );
}
