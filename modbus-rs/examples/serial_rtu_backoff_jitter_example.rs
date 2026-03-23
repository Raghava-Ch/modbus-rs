use anyhow::Result;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, TimeKeeper, UnitIdOrSlaveAddr,
};
use mbus_serial::StdSerialTransport;
use modbus_client::app::{CoilResponse, RequestErrorNotifier};
use modbus_client::services::{ClientServices, coil::Coils};
use std::env;
use std::str::FromStr;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

static RNG_STATE: AtomicU32 = AtomicU32::new(0x1234_5678);

/// Example RNG callback used for bounded jitter.
fn app_random_u32() -> u32 {
    let mut old = RNG_STATE.load(Ordering::Relaxed);
    loop {
        let new = old.wrapping_mul(1103515245).wrapping_add(12345);
        match RNG_STATE.compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return new,
            Err(current) => old = current,
        }
    }
}

#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        println!(
            "[OK] txn={} unit={} read coils from {} qty={} bytes={:02X?}",
            txn_id,
            unit_id.get(),
            coils.from_address(),
            coils.quantity(),
            coils.values()
        );
    }

    fn read_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl RequestErrorNotifier for ClientApp {
    fn request_failed(&self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
        println!(
            "[FAIL] txn={} unit={} error={:?}",
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
    let port_path = args.get(1).map(String::as_str).unwrap_or("/dev/ttyUSB0");
    let unit_id = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1u8);

    println!("--- Modbus Serial RTU Backoff + Jitter Example ---");
    println!("Serial Port: {} unit={}", port_path, unit_id);

    let transport = StdSerialTransport::new(SerialMode::Rtu);
    let app = ClientApp;

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(port_path)
            .map_err(|_| MbusError::BufferTooSmall)?,
        mode: SerialMode::Rtu,
        baud_rate: BaudRate::Baud9600,
        data_bits: mbus_core::transport::DataBits::Eight,
        stop_bits: 1,
        parity: Parity::None,
        response_timeout_ms: 500,
        retry_attempts: 3,
        retry_backoff_strategy: BackoffStrategy::Fixed { delay_ms: 250 },
        retry_jitter_strategy: JitterStrategy::BoundedMs { max_jitter_ms: 40 },
        retry_random_fn: Some(app_random_u32),
    };

    let mut client =
        ClientServices::<_, _, 1>::new(transport, app, ModbusConfig::Serial(serial_config))?;

    let unit = UnitIdOrSlaveAddr::new(unit_id)?;
    client.read_multiple_coils(1, unit, 0, 8)?;

    // poll() drives receive + timeout + scheduled retry flow.
    for _ in 0..120 {
        client.poll();
        sleep(Duration::from_millis(50));
    }

    Ok(())
}
