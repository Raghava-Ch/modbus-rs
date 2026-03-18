use anyhow::Result;
use modbus_client::app::{DiscreteInputResponse, RequestErrorNotifier};
use modbus_client::services::{ClientServices, discrete_input::DiscreteInputs};
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, TimeKeeper, UnitIdOrSlaveAddr};
use mbus_tcp::StdTcpTransport;
use std::env;

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl RequestErrorNotifier for ClientApp {
    fn request_failed(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "Error [Txn: {}, Unit: {}]: Request failed: {:?}",
            txn_id, unit_id.get(), error
        );
    }
}

impl DiscreteInputResponse for ClientApp {
    fn read_discrete_inputs_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, inputs: &DiscreteInputs) {
        let quantity = inputs.quantity();
        println!(
            "Response [Txn: {}, Unit: {}]: Read Discrete Inputs (Addr: {}, Qty: {}):",
            txn_id,
            unit_id.get(),
            inputs.from_address(),
            quantity
        );
        for i in 0..quantity {
            let addr = inputs.from_address() + i;
            match inputs.value(addr) {
                Ok(val) => println!("  Input {}: {}", addr, val),
                Err(e) => println!("  Input {}: Error accessing value: {:?}", addr, e),
            }
        }
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Discrete Input (Addr: {}): {}",
            txn_id, unit_id.get(), address, value
        );
    }
}

impl TimeKeeper for ClientApp {
    fn current_millis(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let host = if args.len() > 1 {
        &args[1]
    } else {
        "192.168.55.105"
    };
    let port = if args.len() > 2 {
        args[2].parse().unwrap_or(502)
    } else {
        502
    };

    println!("--- Modbus Discrete Inputs Example ---");
    println!("Connecting to Modbus TCP Server at {}:{}", host, port);

    let transport = StdTcpTransport::new();
    let app = ClientApp::default();
    let mut tcp_config =
        ModbusTcpConfig::new(host, port).map_err(|e| anyhow::anyhow!(MbusError::from(e)))?;
    tcp_config.connection_timeout_ms = 2000;
    tcp_config.response_timeout_ms = 2000;
    let config = ModbusConfig::Tcp(tcp_config);

    let mut client =
        ClientServices::<_, _, 10>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();

    // 1. Read Single Discrete Input
    println!("\n[1] Sending Read Single Discrete Input (Addr: 0)...");
    client
        .read_single_discrete_input(1, unit_id, 0)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 2. Read Multiple Discrete Inputs
    println!("\n[2] Sending Read Discrete Inputs (Addr: 0, Qty: 10)...");
    client
        .read_discrete_inputs(2, unit_id, 0, 10)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    println!("\n--- Example Completed ---");
    Ok(())
}
