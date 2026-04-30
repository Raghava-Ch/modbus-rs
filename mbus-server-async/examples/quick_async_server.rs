use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server_async::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};

#[derive(Clone)]
struct DemoApp;

#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncTrafficNotifier for DemoApp {}

impl AsyncAppHandler for DemoApp {
    async fn handle(&mut self, _req: ModbusRequest) -> ModbusResponse {
        ModbusResponse::NoResponse
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Quick-start example: bind and serve forever.
    let unit = UnitIdOrSlaveAddr::try_from(1u8)?;
    let _never = AsyncTcpServer::serve("0.0.0.0:1502", DemoApp, unit).await?;
    Ok(())
}
