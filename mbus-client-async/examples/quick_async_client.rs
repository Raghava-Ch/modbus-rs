use mbus_client_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> Result<(), mbus_client_async::AsyncError> {
    // Quick-start example: create client, connect, send one request.
    let client = AsyncTcpClient::new("127.0.0.1", 502)?;
    client.connect().await?;

    let _coils = client.read_multiple_coils(1, 0, 8).await?;

    Ok(())
}
