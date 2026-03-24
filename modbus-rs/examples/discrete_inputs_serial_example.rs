use anyhow::Result;
use modbus_rs::{
    BackoffStrategy, BaudRate, ClientServices, DataBits, DiscreteInputResponse, DiscreteInputs,
    JitterStrategy, MbusError, ModbusConfig, ModbusSerialConfig, Parity, RequestErrorNotifier, SerialMode,
    StdSerialTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl DiscreteInputResponse for ClientApp {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        inputs: &DiscreteInputs,
    ) {
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
                Err(e) => println!("  Input {}: Error: {:?}", addr, e),
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
            txn_id,
            unit_id.get(),
            address,
            value
        );
    }
}

impl RequestErrorNotifier for ClientApp {
    fn request_failed(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "Error [Txn: {}, Unit: {}]: Request failed: {:?}",
            txn_id,
            unit_id.get(),
            error
        );
    }
}
impl TimeKeeper for ClientApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    let port_path = if args.len() > 1 {
        &args[1]
    } else {
        "/dev/ttyUSB0"
    };
    let unit_id_val = if args.len() > 2 {
        args[2].parse().unwrap_or(1)
    } else {
        1
    };

    println!("--- Modbus Serial Discrete Inputs Example ---");
    println!("Connecting to Serial Port: {}", port_path);

    let transport = StdSerialTransport::new(SerialMode::Rtu);
    let app = ClientApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(port_path).unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 2000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client =
        ClientServices::<_, _, 1>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let target_unit_id = UnitIdOrSlaveAddr::try_from(unit_id_val).unwrap();

    // 1. Read Discrete Inputs
    println!("\n[1] Sending Read Discrete Inputs (Addr: 0, Qty: 10)...");
    client.discrete_inputs().read_discrete_inputs(1, target_unit_id, 0, 10)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
