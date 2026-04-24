# mbus-server

Modbus server runtime for Rust — derive-based data models with compile-time address validation.

[![crates.io](https://img.shields.io/crates/v/mbus-server)](https://crates.io/crates/mbus-server)
[![docs.rs](https://docs.rs/mbus-server/badge.svg)](https://docs.rs/mbus-server)

## Features

- **Derive macros** — `CoilsModel`, `HoldingRegistersModel`, `InputRegistersModel`, `DiscreteInputsModel`
- **FIFO service** — `fifo` feature (FC18 Read FIFO Queue)
- **File Record service** — `file-record` feature (FC14 Read File Record, FC15 Write File Record)
- **Compile-time checks** — address overlap detection, range validation
- **Write hooks** — approve/reject writes per field or batch
- **All standard FCs** — 19 function codes supported
- **no_std compatible** — runs on embedded MCUs

## Quick Start

```rust
use mbus_server::{HoldingRegistersModel, modbus_app};

#[derive(Debug, Clone, Default, HoldingRegistersModel)]
struct Registers {
    #[reg(addr = 0, scale = 0.1, unit = "°C")]
    temperature: u16,
    #[reg(addr = 1)]
    setpoint: u16,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(regs))]
struct App {
    regs: Registers,
}

// `#[modbus_app]` auto-implements enabled handler traits for selected maps.
// For `holding_registers(regs)`, this includes FC03/FC06/FC10/FC16/FC17 handlers.
// Additional groups are also supported:
// - `fifo(queue_a, queue_b)` routes FC18 by `FifoQueue::POINTER_ADDRESS`
// - `file_record(files)` routes FC14/FC15 by `FileRecord::FILE_NUMBER`
```

If you want macro-generated FIFO or file-record routing, each listed field must
implement `FifoQueue` or `FileRecord` respectively.

### Optional Traffic Callbacks (`traffic` feature)

```rust
use mbus_core::{MbusError, UnitIdOrSlaveAddr};
use mbus_server::app::TrafficNotifier;

impl TrafficNotifier for App {
    fn on_rx_frame(&mut self, _txn_id: u16, _uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("RX {} bytes", frame.len());
    }

    fn on_tx_frame(&mut self, _txn_id: u16, _uid: UnitIdOrSlaveAddr, frame: &[u8]) {
        println!("TX {} bytes", frame.len());
    }

    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        err: MbusError,
        frame: &[u8],
    ) {
        println!("RX error {:?} on {} bytes", err, frame.len());
    }

    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        err: MbusError,
        frame: &[u8],
    ) {
        println!("TX error {:?} on {} bytes", err, frame.len());
    }
}
```

## Documentation

Use the workspace docs for full server guides:

- [README.md](../documentation/server/README.md) — server docs entry point
- [quick_start.md](../documentation/server/quick_start.md)
- [macros.md](../documentation/server/macros.md)
- [write_hooks.md](../documentation/server/write_hooks.md)
- [function_codes.md](../documentation/server/function_codes.md)
- [feature_flags.md](../documentation/server/feature_flags.md)
- [examples.md](../documentation/server/examples.md)
- [architecture.md](../documentation/server/architecture.md)
- [policies.md](../documentation/server/policies.md)
- [building_applications.md](../documentation/server/building_applications.md)

## Supported Function Codes

| FC | Name | Feature |
|----|------|---------|
| `0x01` | Read Coils | `coils` |
| `0x02` | Read Discrete Inputs | `discrete-inputs` |
| `0x03` | Read Holding Registers | `holding-registers` |
| `0x04` | Read Input Registers | `input-registers` |
| `0x05` | Write Single Coil | `coils` |
| `0x06` | Write Single Register | `holding-registers` |
| `0x0F` | Write Multiple Coils | `coils` |
| `0x10` | Write Multiple Registers | `holding-registers` |
| `0x17` | Read/Write Multiple | `holding-registers` |
| `0x2B` | Device Identification | `diagnostics` |

[See all 19 FCs ->](../documentation/server/function_codes.md)

## Related Crates

| Crate | Purpose |
|-------|---------|
| [`modbus-rs`](https://crates.io/crates/modbus-rs) | Top-level convenience crate |
| [`mbus-core`](https://crates.io/crates/mbus-core) | Shared protocol types |
| [`mbus-client`](https://crates.io/crates/mbus-client) | Client state machine |

## License

This crate is licensed under **GPL-3.0-only**.

If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.
