use anyhow::Result;
use modbus_rs::{
    BackoffStrategy, BaudRate, ClientServices, DataBits, JitterStrategy, MbusError, ModbusConfig,
    ModbusSerialConfig, Parity, RegisterResponse, Registers, RequestErrorNotifier, SerialMode,
    StdRtuTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl RegisterResponse for ClientApp {
    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Input Registers (Addr: {}, Qty: {}): {:?}",
            txn_id,
            unit_id.get(),
            registers.from_address(),
            registers.quantity(),
            registers.values()
        );
    }
    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Holding Registers (Addr: {}, Qty: {}): {:?}",
            txn_id,
            unit_id.get(),
            registers.from_address(),
            registers.quantity(),
            registers.values()
        );
    }
    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Single Register (Addr: {}, Value: {}) Success",
            txn_id,
            unit_id.get(),
            address,
            value
        );
    }
    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Multiple Registers (Addr: {}, Qty: {}) Success",
            txn_id,
            unit_id.get(),
            starting_address,
            quantity
        );
    }
    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read/Write Multiple Registers: {:?}",
            txn_id,
            unit_id.get(),
            registers.values()
        );
    }
    fn read_single_input_register_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
    ) {
    }
    fn read_single_holding_register_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
    ) {
    }
    fn mask_write_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr) {}
    fn read_single_register_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
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

    println!("--- Modbus Serial Registers Example ---");
    println!("Connecting to Serial Port: {}", port_path);

    let transport = StdRtuTransport::new();
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
    client.connect().map_err(|e| anyhow::anyhow!(e))?;

    let target_unit_id = UnitIdOrSlaveAddr::try_from(unit_id_val).unwrap();

    // 1. Write Single Register
    println!("\n[1] Sending Write Single Register (Addr: 10, Val: 1234)...");
    client
        .registers()
        .write_single_register(1, target_unit_id, 10, 1234)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    // 2. Read Holding Registers
    println!("\n[2] Sending Read Holding Registers (Addr: 10, Qty: 5)...");
    client
        .registers()
        .read_holding_registers(2, target_unit_id, 10, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    // 3. Read Input Registers
    println!("\n[3] Sending Read Input Registers (Addr: 0, Qty: 5)...");
    client
        .registers()
        .read_input_registers(3, target_unit_id, 0, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
