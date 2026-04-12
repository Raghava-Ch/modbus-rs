//! Demonstrates `#[derive(HoldingRegistersModel)]` and `#[modbus_app]` for FC03 routing.
//!
//! Two independent register maps are wired into a single application struct.
//! Address-range dispatch is generated automatically by `#[modbus_app]`.
//!
//! Run: cargo run -p mbus-server --example read_multiple_holding_registers_request

use std::sync::{Arc, Mutex};

use heapless::Vec;
use mbus_core::data_unit::common::{self, MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN, Pdu};
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::{
    ModbusConfig, ModbusTcpConfig, Transport, TransportError, TransportType, UnitIdOrSlaveAddr,
};
use mbus_server::{HoldingRegistersModel, ResilienceConfig, ServerServices, modbus_app};

// ---------------------------------------------------------------------------
// Register map 1 — chiller loop (addresses 0-3)
// ---------------------------------------------------------------------------

/// Wire-format holding-register view of the chiller loop.
///
/// Each field is stored as a raw `u16` word.  Scale / unit conversion is the
/// caller's responsibility.  Generated getters / setters allow ergonomic access
/// without exposing the private wire storage.
#[derive(Debug, Clone, Default, HoldingRegistersModel)]
struct ChillerRegisters {
    /// Supply water temperature × 10  (e.g. 215 = 21.5 °C)
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    supply_temp: u16,
    /// Return water temperature × 10  (e.g. 280 = 28.0 °C)
    #[reg(addr = 1, scale = 0.1, unit = "C")]
    return_temp: u16,
    /// Operating mode code (0 = off, 1 = cool, 2 = heat, 3 = auto)
    #[reg(addr = 2)]
    operating_mode: u16,
    /// Active alarm bitmap  (0 = no active alarms)
    #[reg(addr = 3)]
    active_alarm: u16,
}

// ---------------------------------------------------------------------------
// Register map 2 — compressor metrics (addresses 100-102)
// ---------------------------------------------------------------------------

/// Wire-format holding-register view of the compressor metrics.
#[derive(Debug, Clone, Default, HoldingRegistersModel)]
struct CompressorRegisters {
    /// Discharge pressure × 10  (kPa)
    #[reg(addr = 100, scale = 0.1, unit = "kPa")]
    discharge_pressure: u16,
    /// Suction pressure × 10  (kPa)
    #[reg(addr = 101, scale = 0.1, unit = "kPa")]
    suction_pressure: u16,
    /// Compressor shaft speed (rpm)
    #[reg(addr = 102)]
    rpm: u16,
}

// ---------------------------------------------------------------------------
// Application struct
// ---------------------------------------------------------------------------

/// Plant controller that serves both register maps under one FC03 handler.
///
/// `#[modbus_app]` re-emits this struct unchanged and generates a
/// `ModbusAppHandler` impl directly on it with FC03 address-range dispatch.
/// The response buffer lives on the stack inside `ServerServices::poll()` —
/// no permanent per-struct RAM overhead.
///
/// Pass `PlantControllerApp` directly to [`ServerServices::new()`].
#[derive(Debug, Default)]
#[modbus_app(holding_registers(chiller, compressor))]
struct PlantControllerApp {
    chiller: ChillerRegisters,
    compressor: CompressorRegisters,
}

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

    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

/// Build a raw FC03 ADU that reads `qty` registers starting at `start_addr`.
fn build_fc03_request(start_addr: u16, qty: u16) -> Vec<u8, MAX_ADU_FRAME_LEN> {
    let hi_addr = (start_addr >> 8) as u8;
    let lo_addr = start_addr as u8;
    let hi_qty = (qty >> 8) as u8;
    let lo_qty = qty as u8;
    let payload: Vec<u8, MAX_PDU_DATA_LEN> =
        Vec::from_slice(&[hi_addr, lo_addr, hi_qty, lo_qty]).expect("valid FC03 payload");
    let pdu = Pdu::new(FunctionCode::ReadHoldingRegisters, payload, 4);
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

    // Build the app and populate register
    // values via generated setters.
    let mut app = PlantControllerApp::default();
    app.chiller
        .set_supply_temp_scaled(21.5)
        .expect("scaled set should fit");
    app.chiller.set_return_temp(280); // raw wire value for 28.0 C
    app.chiller.set_operating_mode(3); // auto
    app.chiller.set_active_alarm(0);
    app.compressor
        .set_discharge_pressure_scaled(185.0)
        .expect("scaled set should fit");
    app.compressor.set_suction_pressure(420); // raw wire value for 42.0 kPa
    app.compressor.set_rpm(3600);

    // Showcase helper-generated APIs on two fields only.
    let _supply_temp_c = app.chiller.supply_temp_scaled();
    let _supply_temp_unit = ChillerRegisters::supply_temp_unit();
    let _discharge_kpa = app.compressor.discharge_pressure_scaled();
    let _discharge_unit = CompressorRegisters::discharge_pressure_unit();

    let mut server = ServerServices::new(transport, app, config, unit_id(1), ResilienceConfig::default());
    server.connect().expect("connect");
    server.poll();

    outbound
        .lock()
        .expect("lock")
        .clone()
        .expect("server should have emitted a FC03 response")
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> Result<(), MbusError> {
    // --- Chiller request: read 4 registers starting at address 0 ---
    let chiller_req = build_fc03_request(0, 4);
    let chiller_resp = run_request(chiller_req);
    let chiller_adu = common::decompile_adu_frame(chiller_resp.as_slice(), TransportType::StdTcp)?;
    println!("=== Chiller (addr 0-3) ===");
    println!("  function code  : {:?}", chiller_adu.pdu.function_code());
    let chiller_data = chiller_adu.pdu.data();
    println!("  raw bytes      : {:02X?}", chiller_data.as_slice());
    // Response PDU data: raw register bytes, no byte-count prefix.
    if chiller_data.len() >= 8 {
        let supply_temp = u16::from_be_bytes([chiller_data[0], chiller_data[1]]);
        let return_temp = u16::from_be_bytes([chiller_data[2], chiller_data[3]]);
        let operating_mode = u16::from_be_bytes([chiller_data[4], chiller_data[5]]);
        let active_alarm = u16::from_be_bytes([chiller_data[6], chiller_data[7]]);
        println!(
            "  supply temp    : {:.1} °C  (raw {})",
            supply_temp as f32 / 10.0,
            supply_temp
        );
        println!(
            "  return temp    : {:.1} °C  (raw {})",
            return_temp as f32 / 10.0,
            return_temp
        );
        println!("  operating mode : {}", operating_mode);
        println!("  active alarm   : {}", active_alarm);
    }

    println!();

    // --- Compressor request: read 3 registers starting at address 100 ---
    let comp_req = build_fc03_request(100, 3);
    let comp_resp = run_request(comp_req);
    let comp_adu = common::decompile_adu_frame(comp_resp.as_slice(), TransportType::StdTcp)?;
    println!("=== Compressor (addr 100-102) ===");
    println!("  function code       : {:?}", comp_adu.pdu.function_code());
    let comp_data = comp_adu.pdu.data();
    println!("  raw bytes           : {:02X?}", comp_data.as_slice());
    if comp_data.len() >= 6 {
        let discharge = u16::from_be_bytes([comp_data[0], comp_data[1]]);
        let suction = u16::from_be_bytes([comp_data[2], comp_data[3]]);
        let rpm = u16::from_be_bytes([comp_data[4], comp_data[5]]);
        println!(
            "  discharge pressure  : {:.1} kPa  (raw {})",
            discharge as f32 / 10.0,
            discharge
        );
        println!(
            "  suction pressure    : {:.1} kPa  (raw {})",
            suction as f32 / 10.0,
            suction
        );
        println!("  rpm                 : {}", rpm);
    }

    Ok(())
}
