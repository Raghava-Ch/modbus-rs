use anyhow::Result;
use modbus_rs::mbus_async::AsyncTcpClient;
use modbus_rs::ModbusTcpConfig;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    let mut tcp_config = ModbusTcpConfig::new("192.168.55.200", 502)?;
    tcp_config.response_timeout_ms = 2000;

    let client = AsyncTcpClient::new_with_config(tcp_config, Duration::from_millis(20))?;

    let mut traffic_rx = client.traffic_watch();
    tokio::spawn(async move {
        while traffic_rx.changed().await.is_ok() {
            if let Some(evt) = traffic_rx.borrow_and_update().clone() {
                if let Some(err) = evt.error {
                    println!(
                        "[{:?}] txn={} unit={} error={:?} bytes={:02X?}",
                        evt.direction,
                        evt.txn_id,
                        evt.unit_id_slave_addr.get(),
                        err,
                        evt.frame
                    );
                } else {
                    println!(
                        "[{:?}] txn={} unit={} bytes={:02X?}",
                        evt.direction,
                        evt.txn_id,
                        evt.unit_id_slave_addr.get(),
                        evt.frame
                    );
                }
            }
        }
    });

    client.connect().await?;

    let _ = client.read_multiple_coils(1, 0, 8).await?;
    Ok(())
}
