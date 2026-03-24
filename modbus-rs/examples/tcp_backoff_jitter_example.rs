use anyhow::Result;
use modbus_rs::{
    BackoffStrategy, ClientServices, CoilResponse, Coils, JitterStrategy, MbusError, ModbusConfig,
    ModbusTcpConfig, RequestErrorNotifier, StdTcpTransport, TimeKeeper, UnitIdOrSlaveAddr,
};
use std::env;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread::sleep;
use std::time::{Duration, SystemTime};

static RNG_STATE: AtomicU32 = AtomicU32::new(0xA5A5_1234);

/// Very small example RNG callback for jitter.
/// Applications should replace this with their platform RNG source.
fn app_random_u32() -> u32 {
    let mut old = RNG_STATE.load(Ordering::Relaxed);
    loop {
        let new = old.wrapping_mul(1664525).wrapping_add(1013904223);
        match RNG_STATE.compare_exchange_weak(old, new, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return new,
            Err(current) => old = current,
        }
    }
}

#[derive(Debug, Default)]
struct ClientApp;

impl CoilResponse for ClientApp {
    fn read_coils_response(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, coils: &Coils) {
        println!(
            "[OK] txn={} unit={} read coils from {} qty={} bytes={:02X?}",
            txn_id,
            unit_id.get(),
            coils.from_address(),
            coils.quantity(),
            coils.values()
        );
    }

    fn read_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_single_coil_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: bool) {}
    fn write_multiple_coils_response(&mut self, _: u16, _: UnitIdOrSlaveAddr, _: u16, _: u16) {}
}

impl RequestErrorNotifier for ClientApp {
    fn request_failed(&mut self, txn_id: u16, unit_id: UnitIdOrSlaveAddr, error: MbusError) {
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
    let host = args.get(1).map(String::as_str).unwrap_or("127.0.0.1");
    let port = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(502);
    let unit_id = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1u8);

    println!("--- Modbus TCP Backoff + Jitter Example ---");
    println!("Target: {}:{} unit={} ", host, port, unit_id);

    let transport = StdTcpTransport::new();
    let app = ClientApp;

    let mut tcp_config = ModbusTcpConfig::new(host, port)?;
    tcp_config.connection_timeout_ms = 1500;
    tcp_config.response_timeout_ms = 400;
    tcp_config.retry_attempts = 4;
    tcp_config.retry_backoff_strategy = BackoffStrategy::Exponential {
        base_delay_ms: 120,
        max_delay_ms: 2000,
    };
    tcp_config.retry_jitter_strategy = JitterStrategy::Percentage { percent: 20 };
    tcp_config.retry_random_fn = Some(app_random_u32);

    let mut client = ClientServices::<_, _, 8>::new(transport, app, ModbusConfig::Tcp(tcp_config))?;

    let unit = UnitIdOrSlaveAddr::new(unit_id)?;
    client.coils().read_multiple_coils(1, unit, 0, 8)?;

    // poll() drives receive + timeout + scheduled retry flow.
    for _ in 0..120 {
        client.poll();
        sleep(Duration::from_millis(50));
    }

    Ok(())
}
