use anyhow::Result;
use modbus_rs::{ModbusConfig, ModbusTcpConfig, StdTcpTransport, Transport};

fn main() -> Result<()> {
    // Initialize a logger backend for the `log` facade.
    // Example run:
    // RUST_LOG=debug cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,logging
    // Filter only internal client state-machine events:
    // RUST_LOG=modbus_client=trace cargo run -p modbus-rs --example logging_example --no-default-features --features tcp,client,logging
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut transport = StdTcpTransport::new();

    // Intentionally use a likely-invalid address to demonstrate transport logs.
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("invalid-host.local", 502)?);

    match transport.connect(&config) {
        Ok(()) => {
            println!("Connected successfully");
            let _ = transport.disconnect();
        }
        Err(err) => {
            println!("Connection failed (expected in demo): {err}");
        }
    }

    Ok(())
}
