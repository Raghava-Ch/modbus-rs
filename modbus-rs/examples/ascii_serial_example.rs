use anyhow::Result;
use mbus_core::app::{
    CoilResponse, Coils, DiagnosticsResponse, DiscreteInputResponse, FifoQueueResponse,
    FileRecordResponse, RegisterResponse, RequestErrorNotifier,
};
use mbus_core::client::services::ClientServices;
use mbus_core::client::services::diagnostics::DeviceIdentificationResponse;
use mbus_core::client::services::discrete_inputs::DiscreteInputs;
use mbus_core::client::services::fifo::FifoQueue;
use mbus_core::client::services::file_record::SubRequestParams;
use mbus_core::client::services::registers::Registers;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    BaudRate, ModbusConfig, ModbusSerialConfig, Parity, SerialMode, TimeKeeper,
};
use mbus_serial::StdSerialTransport;
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: u8, coils: &Coils, quantity: u16) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Coils (Addr: {}, Qty: {}):",
            txn_id,
            unit_id,
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
    fn read_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Coil (Addr: {}): {}",
            txn_id, unit_id, address, value
        );
    }
    fn write_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Single Coil (Addr: {}, Value: {}) Success",
            txn_id, unit_id, address, value
        );
    }
    fn write_multiple_coils_response(&self, txn_id: u16, unit_id: u8, address: u16, quantity: u16) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Multiple Coils (Addr: {}, Qty: {}) Success",
            txn_id, unit_id, address, quantity
        );
    }
}

// Implement other required traits with empty/default logic as they are not used in this example.
impl RegisterResponse for ClientApp {
    /// Handles a Read Input Registers response. Not used in this example.
    fn read_input_register_response(&mut self, _: u16, _: u8, _: &Registers) {}
    /// Handles a Read Single Input Register response. Not used in this example.
    fn read_single_input_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    /// Handles a Read Holding Registers response. Not used in this example.
    fn read_holding_registers_response(&mut self, _: u16, _: u8, _: &Registers) {}
    /// Handles a Read Single Holding Register response. Not used in this example.
    fn read_single_holding_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    /// Handles a Write Single Register response. Not used in this example.
    fn write_single_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    /// Handles a Write Multiple Registers response. Not used in this example.
    fn write_multiple_registers_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    /// Handles a Read/Write Multiple Registers response. Not used in this example.
    fn read_write_multiple_registers_response(&mut self, _: u16, _: u8, _: &Registers) {}
    /// Handles a Mask Write Register response. Not used in this example.
    fn mask_write_register_response(&mut self, _: u16, _: u8) {}
    /// Handles a Read Single Register response. Not used in this example.
    fn read_single_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
}
impl RequestErrorNotifier for ClientApp {
    fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError) {
        println!(
            "Error [Txn: {}, Unit: {}]: Request failed: {:?}",
            txn_id, unit_id, error
        );
    }
}
impl FifoQueueResponse for ClientApp {
    /// Handles a Read FIFO Queue response. Not used in this example.
    fn read_fifo_queue_response(&mut self, _: u16, _: u8, _: &FifoQueue) {}
}
impl FileRecordResponse for ClientApp {
    /// Handles a Read File Record response. Not used in this example.
    fn read_file_record_response(&mut self, _: u16, _: u8, _: &[SubRequestParams]) {}
    /// Handles a Write File Record response. Not used in this example.
    fn write_file_record_response(&mut self, _: u16, _: u8) {}
}
impl DiscreteInputResponse for ClientApp {
    /// Handles a Read Discrete Inputs response. Not used in this example.
    fn read_discrete_inputs_response(&mut self, _: u16, _: u8, _: &DiscreteInputs, _: u16) {}
    /// Handles a Read Single Discrete Input response. Not used in this example.
    fn read_single_discrete_input_response(&mut self, _: u16, _: u8, _: u16, _: bool) {}
}
impl TimeKeeper for ClientApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}
impl DiagnosticsResponse for ClientApp {
    fn read_device_identification_response(&self, _: u16, _: u8, _: &DeviceIdentificationResponse) {
    }
    fn encapsulated_interface_transport_response(
        &self,
        _: u16,
        _: u8,
        _: mbus_core::function_codes::public::EncapsulatedInterfaceType,
        _: &[u8],
    ) {
    }
    fn diagnostics_response(&self, _: u16, _: u8, _: u16, _: &[u16]) {}
    fn get_comm_event_counter_response(&self, _: u16, _: u8, _: u16, _: u16) {}
    fn get_comm_event_log_response(&self, _: u16, _: u8, _: u16, _: u16, _: u16, _: &[u8]) {}
    fn read_exception_status_response(&self, _: u16, _: u8, _: u8) {}
    fn report_server_id_response(&self, _: u16, _: u8, _: &[u8]) {}
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
        data_bits: 7,
        stop_bits: 1,
        parity: Parity::Even,
        response_timeout_ms: 2000,
        mode: SerialMode::Ascii,
        retry_attempts: 3,
    };
    let config = ModbusConfig::Serial(serial_config);

    let mut client =
        ClientServices::<_, _, 1>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    // 1. Read Coils
    println!("\n[1] Sending Read Coils (Addr: 0, Qty: 5)...");
    client
        .read_multiple_coils(1, unit_id_val, 0, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
