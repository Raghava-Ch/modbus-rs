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
    fn read_input_register_response(&mut self, _txn_id: u16, _unit_id: u8, _registers: &Registers) {
    }
    fn read_single_input_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn read_holding_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
    }
    fn read_single_holding_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn write_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
    }
    fn write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _starting_address: u16,
        _quantity: u16,
    ) {
    }
    fn read_write_multiple_registers_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _registers: &Registers,
    ) {
    }
    fn mask_write_register_response(&mut self, _txn_id: u16, _unit_id: u8) {}
    fn read_single_register_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _address: u16,
        _value: u16,
    ) {
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

impl FileRecordResponse for ClientApp {
    fn read_file_record_response(
        &mut self,
        _txn_id: u16,
        _unit_id: u8,
        _data: &[SubRequestParams],
    ) {
    }
    fn write_file_record_response(&mut self, _txn_id: u16, _unit_id: u8) {}
}

impl DiscreteInputResponse for ClientApp {
    fn read_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        inputs: &DiscreteInputs,
        quantity: u16,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Discrete Inputs (Addr: {}, Qty: {}):",
            txn_id,
            unit_id,
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
        unit_id: u8,
        address: u16,
        value: bool,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Single Discrete Input (Addr: {}): {}",
            txn_id, unit_id, address, value
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

    let unit_id = 1;

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
