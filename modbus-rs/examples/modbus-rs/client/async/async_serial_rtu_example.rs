use anyhow::Result;
use modbus_rs::mbus_async::AsyncSerialClient;
use modbus_rs::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
};
use std::str::FromStr;

#[tokio::main]
async fn main() -> Result<()> {
    let port_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/dev/ttyUSB0".to_string());
    let unit_id = std::env::args()
        .nth(2)
        .and_then(|v| v.parse::<u8>().ok())
        .unwrap_or(1);

    println!(
        "Preparing serial RTU client for port {} (unit {})",
        port_path, unit_id
    );

    let serial_config = ModbusSerialConfig {
        port_path: heapless::String::<64>::from_str(&port_path)
            .map_err(|_| anyhow::anyhow!("serial port path too long for ModbusSerialConfig"))?,
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

    let client = AsyncSerialClient::new_rtu(serial_config)?;
    client.connect().await?;

    let coils = client.read_multiple_coils(unit_id, 0, 8).await?;
    println!(
        "Read {} coils starting at {}",
        coils.quantity(),
        coils.from_address()
    );
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("  coil[{}] = {}", addr, coils.value(addr)?);
    }

    let holding = client.read_holding_registers(unit_id, 0, 4).await?;
    println!(
        "Read {} holding registers starting at {}",
        holding.quantity(),
        holding.from_address()
    );
    for addr in holding.from_address()..holding.from_address() + holding.quantity() {
        println!("  reg[{}] = {}", addr, holding.value(addr)?);
    }

    Ok(())
}
