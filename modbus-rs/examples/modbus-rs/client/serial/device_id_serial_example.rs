use anyhow::Result;
use modbus_rs::{
    BackoffStrategy, BaudRate, ClientServices, DataBits, DeviceIdentificationResponse,
    DiagnosticSubFunction, DiagnosticsResponse, EncapsulatedInterfaceType, JitterStrategy,
    MbusError, ModbusConfig, ModbusSerialConfig, ObjectId, Parity, ReadDeviceIdCode,
    RequestErrorNotifier, SerialMode, StdRtuTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::env;
use std::str::FromStr;
use std::thread::sleep;
use std::time::{Duration, SystemTime};

// --- Client Application Implementation ---
#[derive(Debug, Default)]
struct ClientApp;

impl DiagnosticsResponse for ClientApp {
    fn read_device_identification_response(
        &mut self,
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
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: EncapsulatedInterfaceType,
        _: &[u8],
    ) {
    }
    fn diagnostics_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: DiagnosticSubFunction,
        _: &[u16],
    ) {
    }
    fn get_comm_event_counter_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
    fn get_comm_event_log_response(
        &mut self,
        _: u16,
        _: UnitIdOrSlaveAddr,
        _: u16,
        _: u16,
        _: u16,
        _: &[u8],
    ) {
    }
    fn read_exception_status_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u8) {}
    fn report_server_id_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: &[u8]) {}
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

    println!("--- Modbus Serial Device ID Example ---");
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

    // 1. Read Basic Device Identification
    println!("\n[1] Sending Read Device Identification (Basic)...");
    client
        .diagnostic()
        .read_device_identification(
            1,
            target_unit_id,
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
