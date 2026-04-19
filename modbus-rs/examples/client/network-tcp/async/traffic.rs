use anyhow::Result;
use modbus_rs::mbus_async::{AsyncClientNotifier, AsyncTcpClient};
use modbus_rs::{MbusError, ModbusTcpConfig, UnitIdOrSlaveAddr};
use std::time::Duration;

struct FrameLogger;

impl AsyncClientNotifier for FrameLogger {
    fn on_tx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("[TX] txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
    }

    fn on_rx_frame(&mut self, txn_id: u16, unit: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("[RX] txn={txn_id} unit={} bytes={frame:02X?}", unit.get());
    }

    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        println!(
            "[TX ERR] txn={txn_id} unit={} error={error:?} bytes={frame:02X?}",
            unit.get()
        );
    }

    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        println!(
            "[RX ERR] txn={txn_id} unit={} error={error:?} bytes={frame:02X?}",
            unit.get()
        );
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut tcp_config = ModbusTcpConfig::new("192.168.55.200", 502)?;
    tcp_config.response_timeout_ms = 2000;

    let client = AsyncTcpClient::new_with_config(tcp_config, Duration::from_millis(20))?;
    client.set_traffic_notifier(FrameLogger);
    client.connect().await?;

    let _ = client.read_multiple_coils(1, 0, 8).await?;
    Ok(())
}
