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
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, TimeKeeper};
use mbus_tcp::StdTcpTransport;
use std::env;

// --- Client Application Implementation ---
// This struct implements the necessary traits to handle responses from the Modbus client.
#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(&self, _txn_id: u16, _unit_id: u8, _coils: &Coils, _quantity: u16) {}
    fn read_single_coil_response(&self, _txn_id: u16, _unit_id: u8, _address: u16, _value: bool) {}
    fn write_single_coil_response(&self, _txn_id: u16, _unit_id: u8, _address: u16, _value: bool) {}
    fn write_multiple_coils_response(
        &self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _quantity: u16,
    ) {
    }
}

impl RegisterResponse for ClientApp {
    fn read_input_register_response(&mut self, txn_id: u16, unit_id: u8, registers: &Registers) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Input Registers (Addr: {}, Qty: {}): {:?}",
            txn_id,
            unit_id,
            registers.from_address(),
            registers.quantity(),
            registers.values()
        );
    }

    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Input Register (Addr: {}): {}",
            txn_id, unit_id, address, value
        );
    }

    fn read_holding_registers_response(&mut self, txn_id: u16, unit_id: u8, registers: &Registers) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Holding Registers (Addr: {}, Qty: {}): {:?}",
            txn_id,
            unit_id,
            registers.from_address(),
            registers.quantity(),
            registers.values()
        );
    }

    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Holding Register (Addr: {}): {}",
            txn_id, unit_id, address, value
        );
    }

    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Single Register (Addr: {}, Value: {}) Success",
            txn_id, unit_id, address, value
        );
    }

    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        starting_address: u16,
        quantity: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Write Multiple Registers (Addr: {}, Qty: {}) Success",
            txn_id, unit_id, starting_address, quantity
        );
    }

    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        registers: &Registers,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read/Write Multiple Registers (Read Addr: {}, Qty: {}): {:?}",
            txn_id,
            unit_id,
            registers.from_address(),
            registers.quantity(),
            registers.values()
        );
    }

    fn mask_write_register_response(&mut self, txn_id: u16, unit_id: u8) {
        println!(
            "Response [Txn: {}, Unit: {}]: Mask Write Register Success",
            txn_id, unit_id
        );
    }

    fn read_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Register (Addr: {}): {}",
            txn_id, unit_id, address, value
        );
    }
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
    fn read_fifo_queue_response(&mut self, _txn_id: u16, _unit_id: u8, _values: &FifoQueue) {}
}

impl DiscreteInputResponse for ClientApp {
    fn read_discrete_inputs_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _inputs: &DiscreteInputs,
        _quantity: u16,
    ) {
    }

    fn read_single_discrete_input_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: bool,
    ) {
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

impl DiagnosticsResponse for ClientApp {
    fn read_device_identification_response(
        &self,
        _txn_id: u16,
        _unit_id: u8,
        _response: &DeviceIdentificationResponse,
    ) {
    }

    fn encapsulated_interface_transport_response(
        &self,
        _txn_id: u16,
        _unit_id: u8,
        _mei_type: mbus_core::function_codes::public::EncapsulatedInterfaceType,
        _data: &[u8],
    ) {
    }
    fn diagnostics_response(&self, _txn_id: u16, _unit_id: u8, _sub_function: u16, _data: &[u16]) {}
    fn get_comm_event_counter_response(
        &self,
        _txn_id: u16,
        _unit_id: u8,
        _status: u16,
        _event_count: u16,
    ) {
    }
    fn get_comm_event_log_response(
        &self,
        _txn_id: u16,
        _unit_id: u8,
        _status: u16,
        _event_count: u16,
        _message_count: u16,
        _events: &[u8],
    ) {
    }
    fn read_exception_status_response(&self, _txn_id: u16, _unit_id: u8, _status: u8) {}
    fn report_server_id_response(&self, _txn_id: u16, _unit_id: u8, _data: &[u8]) {}
}

impl FileRecordResponse for ClientApp {
    fn read_file_record_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _data: &[SubRequestParams],
    ) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
    fn write_file_record_response(&mut self, _txn_id: u16, _unit_id: u8) {
        // For simplicity, we won't implement this in the mock since it's not used in the current tests.
    }
}

// This example demonstrates how to use the Modbus Register Services to interact with a server.
// It performs a series of operations: writing a single register, reading it back,
// writing multiple registers, reading a range of holding and input registers,
// and finally performing a combined Read/Write operation in a single transaction.

fn main() -> Result<()> {
    // Parse command line arguments for host and port
    let args: Vec<String> = env::args().collect();
    let host = if args.len() > 1 {
        &args[1]
    } else {
        "192.168.55.106"
    };
    let port = if args.len() > 2 {
        args[2].parse().unwrap_or(502)
    } else {
        502
    };

    println!("--- Modbus Register Services Example ---");
    println!("Connecting to Modbus TCP Server at {}:{}", host, port);

    // Initialize Transport
    let transport = StdTcpTransport::new();

    // Initialize Application Layer
    let app = ClientApp::default();

    // Configure Modbus
    let mut tcp_config =
        ModbusTcpConfig::new(host, port).map_err(|e| anyhow::anyhow!(MbusError::from(e)))?;
    tcp_config.connection_timeout_ms = 2000;
    tcp_config.response_timeout_ms = 2000;
    let config = ModbusConfig::Tcp(tcp_config);

    // Initialize Client Services
    let mut client =
        ClientServices::<_, _, 10>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = 1;

    // 1. Write Single Register
    // Writes value 1234 to register at address 10
    println!("\n[1] Sending Write Single Register (Addr: 10, Val: 1234)...");
    client
        .write_single_register(1, unit_id, 10, 1234)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll(); // Process response

    // 2. Read Single Holding Register
    // Reads back the value from register at address 10
    println!("\n[2] Sending Read Single Holding Register (Addr: 10)...");
    client
        .read_single_holding_register(2, unit_id, 10)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 3. Write Multiple Registers
    // Writes values [10, 20, 30, 40, 50] starting at address 20
    println!("\n[3] Sending Write Multiple Registers (Addr: 20, Qty: 5)...");
    let values = [10, 20, 30, 40, 50];
    client
        .write_multiple_registers(3, unit_id, 20, 5, &values)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 4. Read Holding Registers
    // Reads 5 registers starting at address 20
    println!("\n[4] Sending Read Holding Registers (Addr: 20, Qty: 5)...");
    client
        .read_holding_registers(4, unit_id, 20, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 5. Read Input Registers
    // Reads 5 input registers starting at address 0
    println!("\n[5] Sending Read Input Registers (Addr: 0, Qty: 5)...");
    client
        .read_input_registers(5, unit_id, 0, 5)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 6. Read/Write Multiple Registers
    // Reads 2 registers at 20, and writes [99, 88] to address 30
    println!("\n[6] Sending Read/Write Multiple Registers (Read: 20/2, Write: 30/2)...");
    let write_vals = [99, 88];
    client
        .read_write_multiple_registers(6, unit_id, 20, 2, 30, &write_vals)
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    println!("\n--- Example Completed ---");
    Ok(())
}
