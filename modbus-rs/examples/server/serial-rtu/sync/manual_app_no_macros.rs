use anyhow::{Context, Result};
use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
#[cfg(feature = "traffic")]
use mbus_server::TrafficNotifier;
use mbus_server::ServerExceptionHandler;
use mbus_server::ServerCoilHandler;
use mbus_server::ServerHoldingRegisterHandler;
use mbus_server::ServerInputRegisterHandler;
use mbus_server::ResilienceConfig;
use mbus_server::ServerServices;
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, MbusError, ModbusConfig,
    ModbusSerialConfig, Parity, SerialMode, StdRtuTransport, UnitIdOrSlaveAddr,
};
use std::str::FromStr;
use std::thread;
use std::time::Duration;

const COIL_COUNT: usize = 32;
const REG_COUNT: usize = 32;

#[derive(Debug)]
struct ManualServerApp {
    coils: [bool; COIL_COUNT],
    holding: [u16; REG_COUNT],
    input: [u16; REG_COUNT],
}

impl Default for ManualServerApp {
    fn default() -> Self {
        Self {
            coils: [false; COIL_COUNT],
            holding: [0; REG_COUNT],
            input: [0; REG_COUNT],
        }
    }
}

impl ManualServerApp {
    fn new_seeded() -> Self {
        let mut app = Self::default();

        app.holding[0] = 220;
        app.holding[1] = 2;

        app.input[0] = 245;
        app.input[1] = 1013;

        app.coils[0] = true;
        app.coils[1] = true;
        app.coils[2] = false;
        app.coils[3] = true;

        app
    }

    fn check_range(start: u16, quantity: u16, max_len: usize) -> Result<(usize, usize), MbusError> {
        if quantity == 0 {
            return Err(MbusError::InvalidQuantity);
        }

        let s = start as usize;
        let e = s.saturating_add(quantity as usize);
        if e > max_len {
            return Err(MbusError::InvalidAddress);
        }

        Ok((s, e))
    }

    fn encode_coils(&self, address: u16, quantity: u16, out: &mut [u8]) -> Result<u8, MbusError> {
        let (start, end) = Self::check_range(address, quantity, self.coils.len())?;
        let byte_count = (quantity as usize).div_ceil(8);
        if out.len() < byte_count {
            return Err(MbusError::BufferTooSmall);
        }

        out[..byte_count].fill(0);
        for (i, bit_addr) in (start..end).enumerate() {
            if self.coils[bit_addr] {
                out[i / 8] |= 1u8 << (i % 8);
            }
        }

        Ok(byte_count as u8)
    }

    fn encode_registers(
        regs: &[u16],
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let (start, end) = Self::check_range(address, quantity, regs.len())?;
        let byte_count = quantity as usize * 2;
        if out.len() < byte_count {
            return Err(MbusError::BufferTooSmall);
        }

        for (i, value) in regs[start..end].iter().enumerate() {
            out[i * 2] = (value >> 8) as u8;
            out[i * 2 + 1] = *value as u8;
        }

        Ok(byte_count as u8)
    }
}

#[cfg(feature = "traffic")]
impl TrafficNotifier for ManualServerApp {}

impl ServerExceptionHandler for ManualServerApp {
    fn on_exception(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        exception_code: ExceptionCode,
        error: MbusError,
    ) {
        eprintln!(
            "exception txn={} unit={} fc={:?} code={:?} error={:?}",
            txn_id,
            unit_id_or_slave_addr.get(),
            function_code,
            exception_code,
            error
        );
    }
}

impl ServerCoilHandler for ManualServerApp {
    fn read_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.encode_coils(address, quantity, out)
    }

    fn write_single_coil_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let (start, _) = Self::check_range(address, 1, self.coils.len())?;
        self.coils[start] = value;
        Ok(())
    }

    fn write_multiple_coils_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        let (start, end) = Self::check_range(starting_address, quantity, self.coils.len())?;
        let needed_bytes = (quantity as usize).div_ceil(8);
        if values.len() < needed_bytes {
            return Err(MbusError::BufferTooSmall);
        }

        for (i, bit_addr) in (start..end).enumerate() {
            let bit = (values[i / 8] >> (i % 8)) & 0x01;
            self.coils[bit_addr] = bit == 1;
        }
        Ok(())
    }
}

impl mbus_server::ServerDiscreteInputHandler for ManualServerApp {}

impl ServerHoldingRegisterHandler for ManualServerApp {
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Self::encode_registers(&self.holding, address, quantity, out)
    }

    fn write_single_register_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let (start, _) = Self::check_range(address, 1, self.holding.len())?;
        self.holding[start] = value;
        Ok(())
    }

    fn write_multiple_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        if values.is_empty() {
            return Err(MbusError::InvalidQuantity);
        }

        let quantity = values.len() as u16;
        let (start, end) = Self::check_range(starting_address, quantity, self.holding.len())?;
        self.holding[start..end].copy_from_slice(values);
        Ok(())
    }
}

impl ServerInputRegisterHandler for ManualServerApp {
    fn read_multiple_input_registers_request(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        Self::encode_registers(&self.input, address, quantity, out)
    }
}

impl mbus_server::ServerFifoHandler for ManualServerApp {}
impl mbus_server::ServerFileRecordHandler for ManualServerApp {}
impl mbus_server::ServerDiagnosticsHandler for ManualServerApp {}

fn unit_id(v: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(v).expect("valid unit id")
}

fn baud_from_u32(v: u32) -> BaudRate {
    match v {
        9600 => BaudRate::Baud9600,
        19200 => BaudRate::Baud19200,
        custom => BaudRate::Custom(custom),
    }
}

fn parse_cli() -> Result<(String, u8, u32)> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|a| a == "--help" || a == "-h" || a == "help")
    {
        println!("Usage: server_serial_rtu_manual_app [--port PATH] [--unit UNIT] [--baud BAUD]");
        std::process::exit(0);
    }

    let mut port = std::env::var("MBUS_SERVER_SERIAL_PORT").unwrap_or_else(|_| {
        if cfg!(windows) {
            "COM3".to_string()
        } else {
            "/dev/ttyUSB0".to_string()
        }
    });
    let mut unit = std::env::var("MBUS_SERVER_UNIT")
        .ok()
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);
    let mut baud = std::env::var("MBUS_SERVER_BAUD")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(19200);

    let mut i = 0usize;
    while i < args.len() {
        match args[i].as_str() {
            "--port" if i + 1 < args.len() => {
                port = args[i + 1].clone();
                i += 2;
            }
            "--unit" if i + 1 < args.len() => {
                unit = args[i + 1].parse::<u8>().context("invalid --unit")?;
                i += 2;
            }
            "--baud" if i + 1 < args.len() => {
                baud = args[i + 1].parse::<u32>().context("invalid --baud")?;
                i += 2;
            }
            other => {
                return Err(anyhow::anyhow!("unknown argument `{other}`"));
            }
        }
    }

    Ok((port, unit, baud))
}

fn main() -> Result<()> {
    let (port_path, unit_raw, baud_raw) = parse_cli()?;

    // NOTE: This manual app logic is transport-agnostic.
    // To run the same app over Serial ASCII instead of RTU, only change transport/config:
    // 1) `mode: SerialMode::Ascii`
    // 2) `StdRtuTransport::new()` -> `StdAsciiTransport::new()`
    // 3) ASCII framing defaults: `data_bits: DataBits::Seven`, `parity: Parity::Even`
    // 4) Build with `serial-ascii` feature (instead of `serial-rtu`).

    let config = ModbusConfig::Serial(ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(&port_path)
            .map_err(|_| anyhow::anyhow!("serial port path too long"))?,
        mode: SerialMode::Rtu,
        baud_rate: baud_from_u32(baud_raw),
        data_bits: DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 100,
        retry_attempts: 1,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    });

    let transport = StdRtuTransport::new();
    let app = ManualServerApp::new_seeded();
    let resilience = ResilienceConfig {
        enable_broadcast_writes: true,
        ..ResilienceConfig::default()
    };

    let mut server = ServerServices::new(transport, app, config, unit_id(unit_raw), resilience);
    server.connect()?;

    println!("Manual Modbus RTU server running on {}", port_path);
    println!("Unit id: {}", unit_raw);
    println!("Baud: {}", baud_raw);
    println!("No derive macros, no modbus_app macro. Manual ModbusAppHandler implementation.");
    println!("To switch this to ASCII, only transport/config changes are required.");
    println!("Supported now: FC01, FC03, FC04, FC05, FC06, FC0F, FC10");

    loop {
        server.poll();
        thread::sleep(Duration::from_millis(1));
    }
}
