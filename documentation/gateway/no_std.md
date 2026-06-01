# Gateway in no_std Environments

## Overview

The `mbus-gateway` core is `no_std` compatible when all std-requiring features
(`async`, `logging`) are disabled. All routing tables, the transaction-ID map,
and the downstream channel rx buffer use `heapless` types with const-generic
capacity parameters.

## Cargo.toml

```toml
[dependencies]
mbus-gateway = { version = "0.13.0", default-features = false }
```

## Providing a Transport

You need to supply your own `Transport` implementation for your target hardware:

```rust
use heapless::Vec;
use modbus_rs::{MbusError, MAX_ADU_FRAME_LEN, Transport, TransportType, ModbusConfig, UnitIdOrSlaveAddr};

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
use modbus_rs::gateway::{DownstreamChannel, GatewayServices, NoopEventHandler, UnitRouteTable};
use modbus_rs::UnitIdOrSlaveAddr;

// Routing table
let mut router: UnitRouteTable<4> = UnitRouteTable::new();
router.add(UnitIdOrSlaveAddr::new(1).unwrap(), 0).unwrap();

// Gateway (N_UPSTREAM=1, N_DOWNSTREAM=1, TXN_SIZE=4, N_PENDING=0)
// Initialize GatewayServices with router, event handler, and response timeout in milliseconds.
let mut gw: GatewayServices<UartTransport, UartTransport, _, _, 1, 1> =
    GatewayServices::new(router, NoopEventHandler, 1000);

// Register upstream and downstream transport channels
gw.add_upstream(upstream_uart).unwrap();
gw.add_downstream(DownstreamChannel::new(downstream_uart)).unwrap();

// Main loop
let mut now_ms = 0; // In your hardware environment, read from a timer
loop {
    let _ = gw.poll(now_ms);
    // Yield / sleep / increment now_ms according to your RTOS task scheduler or hardware timer
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

