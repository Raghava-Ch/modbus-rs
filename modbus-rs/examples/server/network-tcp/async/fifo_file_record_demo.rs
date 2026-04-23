//! Async Modbus TCP server using `fifo(...)` and `file_record(...)` selectors.
//!
//! This example serves a live app over TCP with:
//! - FC18 `Read FIFO Queue` routed to `history`
//! - FC14/FC15 `Read/Write File Record` routed to `alarm_file`
//!
//! Run:
//! `cargo run -p modbus-rs --example modbus_rs_server_async_fifo_file_record_demo --features server,async,network-tcp`
//!
//! Optional environment variables:
//! - `MBUS_SERVER_HOST` default `0.0.0.0`
//! - `MBUS_SERVER_PORT` default `5502`
//! - `MBUS_SERVER_UNIT` default `1`

use anyhow::{Context, Result};
use mbus_async::server::AsyncTcpServer;
use mbus_core::{errors::MbusError, transport::UnitIdOrSlaveAddr};
use mbus_server::{FileRecord, FifoQueue, async_modbus_app};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::{Duration, sleep};

#[derive(Debug)]
struct TemperatureHistory {
    values: [u16; 8],
    count: usize,
}

impl Default for TemperatureHistory {
    fn default() -> Self {
        Self {
            values: [245, 247, 249, 251, 253, 255, 0, 0],
            count: 6,
        }
    }
}

impl TemperatureHistory {
    fn push_sample(&mut self, sample: u16) {
        if self.count < self.values.len() {
            self.values[self.count] = sample;
            self.count += 1;
            return;
        }

        self.values.rotate_left(1);
        self.values[self.values.len() - 1] = sample;
    }
}

impl FifoQueue for TemperatureHistory {
    const POINTER_ADDRESS: u16 = 0x0100;

    fn read_fifo_queue(&mut self, out: &mut [u8]) -> Result<u8, MbusError> {
        let byte_count = 2 + self.count * 2;
        if out.len() < byte_count {
            return Err(MbusError::BufferTooSmall);
        }

        let count_u16 = u16::try_from(self.count).map_err(|_| MbusError::InvalidQuantity)?;
        out[0..2].copy_from_slice(&count_u16.to_be_bytes());

        for (index, value) in self.values[..self.count].iter().enumerate() {
            let offset = 2 + index * 2;
            out[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }

        u8::try_from(byte_count).map_err(|_| MbusError::InvalidQuantity)
    }
}

#[derive(Debug)]
struct AlarmFile {
    words: [u16; 8],
}

impl Default for AlarmFile {
    fn default() -> Self {
        Self {
            words: [0x1001, 0x0000, 0x0001, 0x0032, 0x2002, 0x0001, 0x0002, 0x0045],
        }
    }
}

impl AlarmFile {
    fn update_live_fields(&mut self, scan_counter: u16, active_alarm_count: u16) {
        self.words[1] = scan_counter;
        self.words[3] = active_alarm_count;
        self.words[7] = active_alarm_count.saturating_mul(10);
    }
}

impl FileRecord for AlarmFile {
    const FILE_NUMBER: u16 = 7;

    fn read_record(
        &mut self,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let start = record_number as usize;
        let len = record_length as usize;
        let end = start.saturating_add(len);
        if len == 0 || end > self.words.len() {
            return Err(MbusError::InvalidAddress);
        }

        let byte_count = len * 2;
        if out.len() < byte_count {
            return Err(MbusError::BufferTooSmall);
        }

        for (index, value) in self.words[start..end].iter().enumerate() {
            let offset = index * 2;
            out[offset..offset + 2].copy_from_slice(&value.to_be_bytes());
        }

        u8::try_from(byte_count).map_err(|_| MbusError::InvalidQuantity)
    }

    fn write_record(
        &mut self,
        record_number: u16,
        record_length: u16,
        data: &[u16],
    ) -> Result<(), MbusError> {
        let start = record_number as usize;
        let len = record_length as usize;
        let end = start.saturating_add(len);
        if len == 0 || len != data.len() || end > self.words.len() {
            return Err(MbusError::InvalidQuantity);
        }

        self.words[start..end].copy_from_slice(data);
        Ok(())
    }
}

#[derive(Debug, Default)]
#[async_modbus_app(fifo(history), file_record(alarm_file))]
struct DemoAsyncApp {
    history: TemperatureHistory,
    alarm_file: AlarmFile,
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncTrafficNotifier for DemoAsyncApp {}

fn unit_id(value: u8) -> UnitIdOrSlaveAddr {
    UnitIdOrSlaveAddr::try_from(value).expect("valid unit id")
}

fn parse_cli() -> Result<(String, u16, u8)> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        println!(
            "Usage: modbus_rs_server_async_fifo_file_record_demo [--host HOST] [--port PORT] [--unit UNIT]"
        );
        std::process::exit(0);
    }

    let mut host = std::env::var("MBUS_SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let mut port = std::env::var("MBUS_SERVER_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(5502);
    let mut unit = std::env::var("MBUS_SERVER_UNIT")
        .ok()
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(1);

    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--host" if index + 1 < args.len() => {
                host = args[index + 1].clone();
                index += 2;
            }
            "--port" if index + 1 < args.len() => {
                port = args[index + 1].parse::<u16>().context("invalid --port")?;
                index += 2;
            }
            "--unit" if index + 1 < args.len() => {
                unit = args[index + 1].parse::<u8>().context("invalid --unit")?;
                index += 2;
            }
            other => {
                return Err(anyhow::anyhow!("unknown argument `{other}`"));
            }
        }
    }

    Ok((host, port, unit))
}

fn seed_app() -> DemoAsyncApp {
    DemoAsyncApp::default()
}

fn spawn_simulation_task(shared: Arc<Mutex<DemoAsyncApp>>) {
    tokio::spawn(async move {
        let mut sample: u16 = 257;
        let mut active_alarms: u16 = 2;
        let mut scan_counter: u16 = 1;

        loop {
            {
                let mut app = shared.lock().await;
                app.history.push_sample(sample);
                app.alarm_file
                    .update_live_fields(scan_counter, active_alarms);
            }

            sample = if sample >= 272 { 248 } else { sample + 1 };
            active_alarms = if active_alarms >= 5 {
                1
            } else {
                active_alarms + 1
            };
            scan_counter = scan_counter.wrapping_add(1);
            sleep(Duration::from_secs(1)).await;
        }
    });
}

#[tokio::main]
async fn main() -> Result<()> {
    let (host, port, unit_raw) = parse_cli()?;
    let bind = format!("{host}:{port}");
    let unit = unit_id(unit_raw);

    println!("Async FIFO/FileRecord Modbus TCP server on {bind} (unit {})", unit.get());
    println!("FC18 FIFO pointer address : 0x{:04X}", TemperatureHistory::POINTER_ADDRESS);
    println!("FC14/FC15 file number    : {}", AlarmFile::FILE_NUMBER);
    println!("File record words        : 0..7");
    println!("Background task updates FIFO samples and alarm metadata every second");

    let shared = Arc::new(Mutex::new(seed_app()));
    spawn_simulation_task(shared.clone());

    AsyncTcpServer::serve_shared(&bind, shared, unit)
        .await
        .context("server error")?;

    Ok(())
}
