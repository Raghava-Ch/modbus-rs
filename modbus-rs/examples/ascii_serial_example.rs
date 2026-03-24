use anyhow::Result;
use modbus_rs::{
    BackoffStrategy, BaudRate, ClientServices, CoilResponse, Coils, DataBits, JitterStrategy,
    MbusError, ModbusConfig, ModbusSerialConfig, Parity, RequestErrorNotifier, SerialMode,
    StdSerialTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        let quantity = coils.quantity();
        println!(
            "Response [Txn: {}, Unit: {}]: Read Coils (Addr: {}, Qty: {}):",
            txn_id,
            unit_id.get(),
            coils.from_address(),
            quantity
        );
        for i in 0..quantity {
            let addr = coils.from_address() + i;
            match coils.value(addr) {
                Ok(val) => println!("  Coil {}: {}", addr, val),
                Err(e) => println!("  Coil {}: Error: {:?}", addr, e),
            }
        }
    }
    fn read_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Coil (Addr: {}): {}",
            txn_id,
            unit_id.get(),
            address,
            value
        );
    }
    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Single Coil (Addr: {}, Value: {}) Success",
            txn_id,
            unit_id.get(),
            address,
            value
        );
    }
    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Multiple Coils (Addr: {}, Qty: {}) Success",
            txn_id,
            unit_id.get(),
            address,
            quantity
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

    println!("--- Modbus Serial ASCII Example ---");
    println!("Connecting to Serial Port: {}", port_path);

    // Initialize transport with ASCII mode
    let transport = StdSerialTransport::new(SerialMode::Ascii);
    let app = ClientApp::default();

    // Configure serial port for standard Modbus ASCII:
    // - 7 Data Bits
    // - Even Parity
    // - 1 Stop Bit
    // - ASCII Mode
    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(port_path).unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: DataBits::Seven,
        stop_bits: 1,
        parity: Parity::Even,
        response_timeout_ms: 2000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client =
        ClientServices::<_, _, 1>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let target_unit_id = UnitIdOrSlaveAddr::try_from(unit_id_val).unwrap();

    // 1. Read Coils
    println!("\n[1] Sending Read Coils (Addr: 0, Qty: 5)...");
    client.coils().read_multiple_coils(1, target_unit_id, 0, 5)
        .map_err(|e| anyhow::anyhow!(e))?;

    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
