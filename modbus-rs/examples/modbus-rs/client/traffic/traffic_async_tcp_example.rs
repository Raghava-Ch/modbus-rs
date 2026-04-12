use anyhow::Result;
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> Result<()> {
    let client = AsyncTcpClient::new("127.0.0.1", 502)?;

    client.set_traffic_handler(|evt| {
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
    });

    client.connect().await?;

    let _ = client.read_multiple_coils(1, 0, 8).await?;
    Ok(())
}
