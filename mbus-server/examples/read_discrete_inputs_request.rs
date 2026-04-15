//! Demonstrates `#[derive(DiscreteInputsModel)]` and `#[modbus_app]` for FC02 routing.
//!
//! Two independent discrete input maps are wired into a single application struct.
//! Address-range dispatch is generated automatically by `#[modbus_app]`.
//!
//! Run: cargo run -p mbus-server --example read_discrete_inputs_request

use std::sync::{Arc, Mutex};

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::{DiscreteInputsModel, ResilienceConfig, ServerServices, modbus_app};

// ---------------------------------------------------------------------------
// Discrete input map 1 — system status (addresses 0-7)
// ---------------------------------------------------------------------------

/// Wire-format discrete input view of system status indicators.
///
/// Discrete inputs are read-only boolean values. Each field represents a
/// single bit in the Modbus address space.
#[derive(Debug, Clone, Default, DiscreteInputsModel)]
struct SystemStatusInputs {
    /// Power supply OK indicator
    #[discrete_input(addr = 0)]
    power_ok: bool,
    /// System running indicator
    #[discrete_input(addr = 1)]
    system_running: bool,
    /// Emergency stop active
    #[discrete_input(addr = 2)]
    emergency_stop: bool,
    /// Door interlock closed
    #[discrete_input(addr = 3)]
    door_closed: bool,
    /// Cooling system active
    #[discrete_input(addr = 4)]
    cooling_active: bool,
    /// Heating system active
    #[discrete_input(addr = 5)]
    heating_active: bool,
    /// Maintenance mode enabled
    #[discrete_input(addr = 6)]
    maintenance_mode: bool,
    /// System ready indicator
    #[discrete_input(addr = 7)]
    system_ready: bool,
}

// ---------------------------------------------------------------------------
// Discrete input map 2 — sensor alerts (addresses 100-107)
// ---------------------------------------------------------------------------

/// Wire-format discrete input view of sensor alert states.
#[derive(Debug, Clone, Default, DiscreteInputsModel)]
struct SensorAlertInputs {
    /// Temperature sensor 1 alert
    #[discrete_input(addr = 100)]
    temp_sensor_1_alert: bool,
    /// Temperature sensor 2 alert
    #[discrete_input(addr = 101)]
    temp_sensor_2_alert: bool,
    /// Pressure sensor alert
    #[discrete_input(addr = 102)]
    pressure_sensor_alert: bool,
    /// Flow sensor alert
    #[discrete_input(addr = 103)]
    flow_sensor_alert: bool,
    /// Vibration sensor alert
    #[discrete_input(addr = 104)]
    vibration_sensor_alert: bool,
    /// Level sensor alert
    #[discrete_input(addr = 105)]
    level_sensor_alert: bool,
    /// Gas detector alert
    #[discrete_input(addr = 106)]
    gas_detector_alert: bool,
    /// Smoke detector alert
    #[discrete_input(addr = 107)]
    smoke_detector_alert: bool,
}

// ---------------------------------------------------------------------------
// Application struct
// ---------------------------------------------------------------------------

/// Plant controller that serves both discrete input maps under one FC02 handler.
///
/// `#[modbus_app]` re-emits this struct unchanged and generates a
/// `ModbusAppHandler` impl directly on it with FC02 address-range dispatch.
/// The response buffer lives on the stack inside `ServerServices::poll()` —
/// no permanent per-struct RAM overhead.
///
/// Pass `PlantMonitorApp` directly to [`ServerServices::new()`].
#[derive(Debug, Default)]
#[modbus_app(discrete_inputs(system_status, sensor_alerts))]
struct PlantMonitorApp {
    system_status: SystemStatusInputs,
    sensor_alerts: SensorAlertInputs,
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for PlantMonitorApp {}

// ---------------------------------------------------------------------------
// Minimal in-memory transport
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
struct DemoTransport {
    inbound: Option<Vec<u8, MAX_ADU_FRAME_LEN>>,
    outbound: Arc<Mutex<Option<Vec<u8, MAX_ADU_FRAME_LEN>>>>,
    connected: bool,
}

impl Transport for DemoTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let frame = Vec::from_slice(adu).map_err(|_| TransportError::BufferTooSmall)?;
        *self.outbound.lock().expect("lock") = Some(frame);
        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        self.inbound.take().ok_or(TransportError::Timeout)
    }

    fn is_connected(&self) -> bool {
        self.connected
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

/// Build a raw FC02 ADU that reads `qty` discrete inputs starting at `start_addr`.
fn build_fc02_request(start_addr: u16, qty: u16) -> Vec<u8, MAX_ADU_FRAME_LEN> {
    let pdu = Pdu::build_read_window(FunctionCode::ReadDiscreteInputs, start_addr, qty)
        .expect("valid FC02 payload");
    common::compile_adu_frame(0x0001, unit_id(1).get(), pdu, TransportType::StdTcp)
        .expect("request ADU should compile")
}

/// Drive the server with a single inbound request and return the response ADU bytes.
fn run_request(request: Vec<u8, MAX_ADU_FRAME_LEN>) -> Vec<u8, MAX_ADU_FRAME_LEN> {
    let outbound = Arc::new(Mutex::new(None));
    let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502).unwrap());

    let transport = DemoTransport {
        inbound: Some(request),
        outbound: Arc::clone(&outbound),
        connected: false,
    };

    // Build the app and populate discrete input values.
    // Discrete inputs are read-only from the Modbus perspective,
    // but the application can update them internally.
    let mut app = PlantMonitorApp::default();

    // Set system status inputs
    app.system_status.power_ok = true;
    app.system_status.system_running = true;
    app.system_status.emergency_stop = false;
    app.system_status.door_closed = true;
    app.system_status.cooling_active = true;
    app.system_status.heating_active = false;
    app.system_status.maintenance_mode = false;
    app.system_status.system_ready = true;

    // Set sensor alert inputs (some alerts active)
    app.sensor_alerts.temp_sensor_1_alert = false;
    app.sensor_alerts.temp_sensor_2_alert = true;
    app.sensor_alerts.pressure_sensor_alert = false;
    app.sensor_alerts.flow_sensor_alert = false;
    app.sensor_alerts.vibration_sensor_alert = true;
    app.sensor_alerts.level_sensor_alert = false;
    app.sensor_alerts.gas_detector_alert = false;
    app.sensor_alerts.smoke_detector_alert = false;

    let mut server = ServerServices::new(
        transport,
        app,
        config,
        unit_id(1),
        ResilienceConfig::default(),
    );
    server.connect().expect("connect");
    server.poll();

    outbound
        .lock()
        .expect("lock")
        .clone()
        .expect("server should have emitted a FC02 response")
}

/// Decode bit-packed discrete input bytes into individual boolean values.
fn decode_bits(data: &[u8], bit_count: usize) -> Vec<bool, 256> {
    let mut bits = Vec::new();
    for i in 0..bit_count {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        if byte_idx < data.len() {
            let bit_value = (data[byte_idx] & (1 << bit_idx)) != 0;
            bits.push(bit_value).ok();
        }
    }
    bits
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<(), MbusError> {
    // --- System status request: read 8 discrete inputs starting at address 0 ---
    let status_req = build_fc02_request(0, 8);
    let status_resp = run_request(status_req);
    let status_adu = common::decompile_adu_frame(status_resp.as_slice(), TransportType::StdTcp)?;
    println!("=== System Status (addr 0-7) ===");
    println!(
        "  function code        : {:?}",
        status_adu.pdu.function_code()
    );
    let status_data = status_adu.pdu.data();
    println!("  raw bytes            : {:02X?}", status_data.as_slice());

    // Decode the bit-packed response (skip byte count prefix)
    let status_bits = decode_bits(&status_data.as_slice()[1..], 8);
    println!("  decoded bits         : {:?}", status_bits.as_slice());
    if status_bits.len() >= 8 {
        println!("  power_ok             : {}", status_bits[0]);
        println!("  system_running       : {}", status_bits[1]);
        println!("  emergency_stop       : {}", status_bits[2]);
        println!("  door_closed          : {}", status_bits[3]);
        println!("  cooling_active       : {}", status_bits[4]);
        println!("  heating_active       : {}", status_bits[5]);
        println!("  maintenance_mode     : {}", status_bits[6]);
        println!("  system_ready         : {}", status_bits[7]);
    }

    println!();

    // --- Sensor alerts request: read 8 discrete inputs starting at address 100 ---
    let alerts_req = build_fc02_request(100, 8);
    let alerts_resp = run_request(alerts_req);
    let alerts_adu = common::decompile_adu_frame(alerts_resp.as_slice(), TransportType::StdTcp)?;
    println!("=== Sensor Alerts (addr 100-107) ===");
    println!(
        "  function code           : {:?}",
        alerts_adu.pdu.function_code()
    );
    let alerts_data = alerts_adu.pdu.data();
    println!(
        "  raw bytes               : {:02X?}",
        alerts_data.as_slice()
    );

    // Decode the bit-packed response (skip byte count prefix)
    let alert_bits = decode_bits(&alerts_data.as_slice()[1..], 8);
    println!("  decoded bits            : {:?}", alert_bits.as_slice());
    if alert_bits.len() >= 8 {
        println!("  temp_sensor_1_alert     : {}", alert_bits[0]);
        println!("  temp_sensor_2_alert     : {}", alert_bits[1]);
        println!("  pressure_sensor_alert   : {}", alert_bits[2]);
        println!("  flow_sensor_alert       : {}", alert_bits[3]);
        println!("  vibration_sensor_alert  : {}", alert_bits[4]);
        println!("  level_sensor_alert      : {}", alert_bits[5]);
        println!("  gas_detector_alert      : {}", alert_bits[6]);
        println!("  smoke_detector_alert    : {}", alert_bits[7]);
    }

    println!();
    println!("Note: Discrete inputs are read-only. The bits are packed LSB-first in each byte.");

    Ok(())
}
