use anyhow::Result;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::DiagnosticSubFunction;
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, TimeKeeper, UnitIdOrSlaveAddr};
use mbus_tcp::StdTcpTransport;
use modbus_client::app::{DiagnosticsResponse, RequestErrorNotifier};
use modbus_client::services::{
    ClientServices,
    diagnostic::{DeviceIdentificationResponse, ObjectId, ReadDeviceIdCode},
};
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
        unit_id: UnitIdOrSlaveAddr,
        response: &DeviceIdentificationResponse,
    ) {
        println!(
            "Response [Txn: {}, Unit: {}]: Read Device Identification",
            txn_id,
            unit_id.get()
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
        _unit_id: UnitIdOrSlaveAddr,
        _mei_type: mbus_core::function_codes::public::EncapsulatedInterfaceType,
        _data: &[u8],
    ) {
    }
    fn diagnostics_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _sub_function: DiagnosticSubFunction,
        _data: &[u16],
    ) {
    }
    fn get_comm_event_counter_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u16,
        _event_count: u16,
    ) {
    }
    fn get_comm_event_log_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u16,
        _event_count: u16,
        _message_count: u16,
        _events: &[u8],
    ) {
    }
    fn read_exception_status_response(
        &self,
        _txn_id: u16,
        _unit_id: UnitIdOrSlaveAddr,
        _status: u8,
    ) {
    }
    fn report_server_id_response(&self, _txn_id: u16, _unit_id: UnitIdOrSlaveAddr, _data: &[u8]) {}
}

// Implement other required traits with minimal/empty logic for this example
// as they are not used in this example.
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
    /// Returns the current monotonic time in milliseconds.
    fn current_millis(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
    }
}

fn main() -> Result<()> {
    // Parse command line arguments for host and port
    let args: Vec<String> = env::args().collect();
    let host = if args.len() > 1 {
        &args[1]
    } else {
        "192.168.55.200"
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
        ClientServices::<_, _, 10>::new(transport, app, config).map_err(|e| anyhow::anyhow!(e))?;

    let unit_id = UnitIdOrSlaveAddr::try_from(1).unwrap();

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
