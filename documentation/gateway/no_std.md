# Gateway in no_std Environments

## Overview

The `mbus-gateway` core is `no_std` compatible when all std-requiring features
(`async`, `logging`) are disabled.  All routing tables, the transaction-ID map,
and the downstream channel rx buffer use `heapless` types with const-generic
capacity parameters.

## Cargo.toml

```toml
[dependencies]
mbus-gateway = { version = "0.8.0", default-features = false }
```

## Providing a Transport

You need to supply your own `Transport` implementation for your target hardware:

```rust
use mbus_core::transport::{Transport, TransportType, ModbusConfig};
use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;

// Example UART-based transport skeleton
pub struct UartTransport {
    // ... hardware handles ...
}

impl Transport for UartTransport {
    type Error = MbusError;
    const TRANSPORT_TYPE: TransportType = TransportType::StdSerial(
        mbus_core::transport::SerialMode::Rtu,
    );

    fn connect(&mut self, _cfg: &ModbusConfig) -> Result<(), Self::Error> {
        // Initialize UART peripheral
        Ok(())
    }
    fn disconnect(&mut self) -> Result<(), Self::Error> { Ok(()) }
    fn send(&mut self, _bytes: &[u8]) -> Result<(), Self::Error> {
        // Transmit bytes over UART
        Ok(())
    }
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        // Return available bytes (non-blocking) or Err(Timeout) if none ready
        Err(MbusError::Timeout)
    }
    fn is_connected(&self) -> bool { true }
}
```

## Building the Gateway

```rust,no_run
use mbus_gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable};
use mbus_core::transport::UnitIdOrSlaveAddr;

// Routing table
let mut router: UnitRouteTable<4> = UnitRouteTable::new();
router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();

// Gateway (N_DOWNSTREAM=1, TXN_SIZE=1)
let mut gw: GatewayServices<UartTransport, UartTransport, _, _, 1, 1> =
    GatewayServices::new(upstream_uart, router, NoopEventHandler);
gw.add_downstream(DownstreamChannel::new(downstream_uart)).unwrap();

// Main loop
loop {
    let _ = gw.poll();
    // Yield / sleep according to your RTOS task scheduler
}
```

## Memory Footprint

For a single-channel gateway with 1 in-flight transaction and a 4-entry
routing table on `thumbv7m-none-eabi` (approximate):

| Structure | Size |
|-----------|------|
| `UnitRouteTable<4>` | ~12 bytes |
| `TxnMap<1>` | ~8 bytes |
| `GatewayServices` rx buffers (2×) | 2 × 260 bytes |
| Total overhead (excluding transports) | ~550 bytes |

The two `MAX_ADU_FRAME_LEN`-sized rx buffers dominate; everything else is
negligible.
