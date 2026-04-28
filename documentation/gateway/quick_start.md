# Modbus Gateway — Quick Start

This page walks you through the two ways to run a Modbus gateway:
**sync (no_std compatible)** and **async (Tokio)**.

## Prerequisites

```toml
[dependencies]
mbus-gateway = "0.8.0"
modbus-rs = { version = "0.8.0", features = ["gateway", "network-tcp", "serial-rtu"] }
```

## Sync: TCP upstream → RTU downstream

```rust,no_run
use modbus_rs::{ModbusTcpConfig, ModbusSerialConfig, StdTcpServerTransport, StdRtuTransport,
                BaudRate, DataBits, Parity, SerialMode};
use mbus_gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;

// 1. Upstream TCP transport (accepts incoming connections)
let tcp_config = ModbusTcpConfig {
    host: "0.0.0.0".into(),
    port: 502,
    response_timeout_ms: 1000,
    connection_timeout_ms: 5000,
};
let mut upstream = StdTcpServerTransport::new();
upstream.connect(&modbus_rs::ModbusConfig::Tcp(tcp_config)).unwrap();

// 2. Downstream RTU transport
let serial_config = ModbusSerialConfig {
    port: "/dev/ttyUSB0".into(),
    baud_rate: BaudRate::Baud9600,
    data_bits: DataBits::Eight,
    parity: Parity::None,
    stop_bits: modbus_rs::transport::StopBits::One,
    response_timeout_ms: 500,
    mode: SerialMode::Rtu,
};
let mut downstream = StdRtuTransport::new();
downstream.connect(&modbus_rs::ModbusConfig::Serial(serial_config)).unwrap();

// 3. Routing: units 1–10 → channel 0
let mut router: UnitRouteTable<10> = UnitRouteTable::new();
for id in 1u8..=10 {
    router.add(UnitIdOrSlaveAddr::new(id).unwrap(), 0).unwrap();
}

// 4. Create and run gateway
let mut gw: GatewayServices<StdTcpServerTransport, StdRtuTransport, _, _, 1> =
    GatewayServices::new(upstream, router, NoopEventHandler);
gw.add_downstream(DownstreamChannel::new(downstream)).unwrap();
gw.set_max_downstream_recv_attempts(1); // blocking serial recv

loop {
    let _ = gw.poll();
}
```

## Async: TCP upstream → TCP downstream

```rust,no_run
use std::sync::Arc;
use tokio::sync::Mutex;
use mbus_gateway::{AsyncTcpGatewayServer, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Connect to downstream server
    let ds = TokioTcpTransport::connect("192.168.1.10:502").await?;
    let shared = Arc::new(Mutex::new(ds));

    // Build route table
    let mut router: UnitRouteTable<10> = UnitRouteTable::new();
    for id in 1u8..=10 {
        router.add(UnitIdOrSlaveAddr::new(id).unwrap(), 0).unwrap();
    }

    // Run the gateway (infinite loop, returns Infallible or I/O error)
    AsyncTcpGatewayServer::serve("0.0.0.0:502", router, vec![shared]).await?;
    Ok(())
}
```
