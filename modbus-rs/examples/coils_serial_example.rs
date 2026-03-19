use anyhow::Result;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    BaudRate, ModbusConfig, ModbusSerialConfig, Parity, SerialMode, TimeKeeper, UnitIdOrSlaveAddr,
};
use mbus_serial::StdSerialTransport;
use modbus_client::app::{CoilResponse, RequestErrorNotifier};
use modbus_client::services::{ClientServices, coil::Coils};
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(
        &self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        coils: &Coils,
        quantity: u16,
    ) {
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
        &self,
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
        &self,
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
        &self,
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
    fn request_failed(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
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

    println!("--- Modbus Serial Coils Example ---");
    println!("Connecting to Serial Port: {}", port_path);

    let transport = StdSerialTransport::new(SerialMode::Rtu);
    let app = ClientApp::default();

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(port_path).unwrap(),
        baud_rate: BaudRate::Baud9600,
        data_bits: 8,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 2000,
        mode: SerialMode::Rtu,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client =
        ClientServices::<_, _, 1>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let target_unit_id = UnitIdOrSlaveAddr::try_from(unit_id_val).unwrap();

    // 1. Write Single Coil
    println!("\n[1] Sending Write Single Coil (Addr: 0, Value: ON)...");
    client
        .write_single_coil(1, target_unit_id, 0, true)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    // 2. Read Coils
    println!("\n[2] Sending Read Coils (Addr: 0, Qty: 5)...");
    client
        .read_multiple_coils(2, target_unit_id, 0, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
