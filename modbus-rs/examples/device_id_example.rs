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
use mbus_core::device_identification::{ObjectId, ReadDeviceIdCode};
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, TimeKeeper};
use mbus_tcp::StdTcpTransport;
use std::env;
use std::time::{SystemTime, UNIX_EPOCH};

// --- Client Application Implementation ---
// This struct implements the necessary traits to handle responses from the Modbus client.
// For this example, we focus on implementing `DiagnosticsResponse`.
#[derive(Debug, Default)]
struct ClientApp;

// Implement DiagnosticsResponse to handle the Device ID response
impl DiagnosticsResponse for ClientApp {
    fn read_device_identification_response(
        &self,
        txn_id: u16,
        unit_id: u8,
        response: &DeviceIdentificationResponse,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Device Identification",
            txn_id, unit_id
        );
        println!("  Conformity Level: {:?}", response.conformity_level);
        println!("  More Follows: {}", response.more_follows);
        println!("  Next Object ID: {}", response.next_object_id);
        println!("  Objects:");

        // Iterate over the objects in the response
        for obj_res in response.objects() {
            match obj_res {
                Ok(obj) => {
                    // Try to convert value bytes to string for display, otherwise print as hex
                    let value_str = std::str::from_utf8(&obj.value)
                        .map(|s| format!("\"{}\"", s))
                        .unwrap_or_else(|_| format!("Hex: {:02X?}", obj.value));

                    println!("    - {}: {}", obj.object_id, value_str);
                }
                Err(e) => {
                    println!("    - Error parsing object: {:?}", e);
                }
            }
        }
        println!("--------------------------------------------------");
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

// Implement other required traits with minimal/empty logic for this example
impl RequestErrorNotifier for ClientApp {
    fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError) {
        println!(
            "Error [Txn: {}, Unit: {}]: Request failed: {:?}",
            txn_id, unit_id, error
        );
    }
}

impl TimeKeeper for ClientApp {
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

impl CoilResponse for ClientApp {
    fn read_coils_response(&self, _: u16, _: u8, _: &Coils, _: u16) {}
    fn read_single_coil_response(&self, _: u16, _: u8, _: u16, _: bool) {}
    fn write_single_coil_response(&self, _: u16, _: u8, _: u16, _: bool) {}
    fn write_multiple_coils_response(&self, _: u16, _: u8, _: u16, _: u16) {}
}

impl RegisterResponse for ClientApp {
    fn read_input_register_response(&mut self, _: u16, _: u8, _: &Registers) {}
    fn read_single_input_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    fn read_holding_registers_response(&mut self, _: u16, _: u8, _: &Registers) {}
    fn read_single_holding_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    fn write_single_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    fn write_multiple_registers_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
    fn read_write_multiple_registers_response(&mut self, _: u16, _: u8, _: &Registers) {}
    fn mask_write_register_response(&mut self, _: u16, _: u8) {}
    fn read_single_register_response(&mut self, _: u16, _: u8, _: u16, _: u16) {}
}

impl FifoQueueResponse for ClientApp {
    fn read_fifo_queue_response(&mut self, _: u16, _: u8, _: &FifoQueue) {}
}

impl FileRecordResponse for ClientApp {
    fn read_file_record_response(&mut self, _: u16, _: u8, _: &[SubRequestParams]) {}
    fn write_file_record_response(&mut self, _: u16, _: u8) {}
}

impl DiscreteInputResponse for ClientApp {
    fn read_discrete_inputs_response(&mut self, _: u16, _: u8, _: &DiscreteInputs, _: u16) {}
    fn read_single_discrete_input_response(&mut self, _: u16, _: u8, _: u16, _: bool) {}
}

fn main() -> Result<()> {
    // Parse command line arguments for host and port
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

    println!("--- Modbus Device Identification Example ---");
    println!("Connecting to Modbus TCP Server at {}:{}", host, port);

    // Initialize Transport and App
    let transport = StdTcpTransport::new();
    let app = ClientApp::default();

    // Configure Modbus
    let mut tcp_config =
        ModbusTcpConfig::new(host, port).map_err(|e| anyhow::anyhow!(MbusError::from(e)))?;
    tcp_config.connection_timeout_ms = 2000;
    tcp_config.response_timeout_ms = 2000;
    let config = ModbusConfig::Tcp(tcp_config);

    // Initialize Client Services
    let mut client =
        ClientServices::<_, 10, _>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = 1;

    // 1. Read Basic Device Identification (Stream Access)
    // Retrieves mandatory objects: VendorName, ProductCode, MajorMinorRevision
    println!("\n[1] Sending Read Device Identification (Basic)...");
    client
        .read_device_identification(1, unit_id, ReadDeviceIdCode::Basic, ObjectId::from(0x00))
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 2. Read Regular Device Identification (Stream Access)
    // Retrieves optional objects: VendorUrl, ProductName, ModelName, UserApplicationName
    println!("\n[2] Sending Read Device Identification (Regular)...");
    client
        .read_device_identification(2, unit_id, ReadDeviceIdCode::Regular, ObjectId::from(0x00))
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 3. Read Extended Device Identification (Stream Access)
    // Retrieves extended/private objects (0x80 - 0xFF)
    println!("\n[3] Sending Read Device Identification (Extended)...");
    client
        .read_device_identification(3, unit_id, ReadDeviceIdCode::Extended, ObjectId::from(0x80))
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    // 4. Read Specific Object (Individual Access)
    // Retrieves a single specific object, e.g., VendorName (0x00)
    println!("\n[4] Sending Read Device Identification (Specific - VendorName)...");
    client
        .read_device_identification(4, unit_id, ReadDeviceIdCode::Specific, ObjectId::from(0x00))
        .map_err(|e| anyhow::anyhow!(e))?;
    client.poll();

    println!("\n--- Example Completed ---");
    Ok(())
}
