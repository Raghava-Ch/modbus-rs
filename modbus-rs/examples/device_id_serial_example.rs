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

        for obj_res in response.objects() {
            match obj_res {
                Ok(obj) => {
                    let value_str = std::str::from_utf8(&obj.value)
                        .map(|s| format!("\"{}\"", s))
                        .unwrap_or_else(|_| format!("Hex: {:02X?}", obj.value));
                    println!("    - {}: {}", obj.object_id, value_str);
                }
                Err(e) => println!("    - Error parsing object: {:?}", e),
            }
        }
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

// Implement other required traits with empty/default logic
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
impl RequestErrorNotifier for ClientApp {
    fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError) {
        println!(
            "Error [Txn: {}, Unit: {}]: Request failed: {:?}",
            txn_id, unit_id, error
        );
    }
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

    println!("--- Modbus Serial Device ID Example ---");
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

    // 1. Read Basic Device Identification
    println!("\n[1] Sending Read Device Identification (Basic)...");
    client
        .read_device_identification(
            1,
            unit_id_val,
            ReadDeviceIdCode::Basic,
            ObjectId::from(0x00),
        )
        .map_err(|e| anyhow::anyhow!(e))?;
    for _ in 0..5 {
        client.poll();
        sleep(Duration::from_millis(200));
    }

    Ok(())
}
