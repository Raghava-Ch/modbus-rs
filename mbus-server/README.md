# mbus-server

`mbus-server` is the Modbus server-side runtime crate.  It provides the protocol
stack, request dispatch, response framing, and the `ModbusAppHandler` callback
trait that application code implements to service requests.

## Supported Function Codes

| FC     | Name                          | Feature flag        |
|--------|-------------------------------|---------------------|
| `0x01` | Read Coils                    | `coils`             |
| `0x02` | Read Discrete Inputs          | `discrete-inputs`   |
| `0x03` | Read Holding Registers        | `holding-registers` |
| `0x04` | Read Input Registers          | `input-registers`   |
| `0x05` | Write Single Coil             | `coils`             |
| `0x06` | Write Single Register         | `holding-registers` |
| `0x07` | Read Exception Status         | `diagnostics`       |
| `0x08` | Diagnostics                   | `diagnostics`       |
| `0x0B` | Get Comm Event Counter        | `diagnostics`       |
| `0x0C` | Get Comm Event Log            | `diagnostics`       |
| `0x0F` | Write Multiple Coils          | `coils`             |
| `0x10` | Write Multiple Registers      | `holding-registers` |
| `0x11` | Report Server ID              | `diagnostics`       |
| `0x14` | Read File Record              | `file-record`       |
| `0x15` | Write File Record             | `file-record`       |
| `0x16` | Mask Write Register           | `holding-registers` |
| `0x17` | Read/Write Multiple Registers | `holding-registers` |
| `0x18` | Read FIFO Queue               | `fifo`              |
| `0x2B` | Read Device Identification    | `diagnostics`       |

## Quick Start

Implement `ModbusAppHandler` on your app struct — only override the callbacks you
need; all methods have sensible defaults:

```rust
use mbus_core::errors::MbusError;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server::ModbusAppHandler;

struct MyApp {
    regs: [u16; 16],
}

impl ModbusAppHandler for MyApp {
    #[cfg(feature = "holding-registers")]
    fn read_multiple_holding_registers_request(
        &mut self,
        _txn_id: u16,
        _uid: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let s = address as usize;
        let e = s + quantity as usize;
        if e > self.regs.len() { return Err(MbusError::InvalidAddress); }
        for (i, &v) in self.regs[s..e].iter().enumerate() {
            out[i * 2] = (v >> 8) as u8;
            out[i * 2 + 1] = v as u8;
        }
        Ok((quantity * 2) as u8)
    }
}
```

See `examples/full_featured_server.rs` for every function code in one place.

## Derive Macros

`mbus-server` re-exports these proc-macro derives for compile-time mapping:

- `CoilsModel` — bit-packed coil map with FC01/FC05/FC0F support
- `HoldingRegistersModel` — register map with FC03/FC06/FC10/FC16/FC17 support
- `InputRegistersModel` — read-only register map for FC04
- `DiscreteInputsModel` — read-only bit map for FC02
- `modbus_app` — wires derives into `ModbusAppHandler` with compile-time overlap checks

### Coils example

```rust
use mbus_server::CoilsModel;

#[derive(Debug, Clone, Default, CoilsModel)]
struct Coils {
    #[coil(addr = 0)]
    run_enable: bool,
    #[coil(addr = 1)]
    fault_reset: bool,
}
```

### Discrete inputs example

```rust
use mbus_server::DiscreteInputsModel;

#[derive(Debug, Clone, Default, DiscreteInputsModel)]
struct SystemInputs {
    #[discrete_input(addr = 0)]
    power_ok: bool,
    #[discrete_input(addr = 1)]
    emergency_stop: bool,
    #[discrete_input(addr = 2)]
    door_closed: bool,
}
```

### Holding registers + app routing example

```rust
use mbus_server::{HoldingRegistersModel, modbus_app};

#[derive(Debug, Clone, Default, HoldingRegistersModel)]
struct ChillerRegs {
    #[reg(addr = 0, scale = 0.1, unit = "C")]
    supply_temp: u16,
    #[reg(addr = 1)]
    return_temp: u16,
}

#[derive(Debug, Default)]
#[modbus_app(
    holding_registers(chiller),
    discrete_inputs(system_inputs)
)]
struct App {
    chiller: ChillerRegs,
    system_inputs: SystemInputs,
}
```

### Generated API

`CoilsModel` generates:
- `CoilMap` implementation with FC01/FC05/FC15 support

`HoldingRegistersModel` generates:
- per-field getter/setter methods (`field_name()` / `set_field_name(u16)`)
- `HoldingRegisterMap` implementation with FC03 `encode()` support
- optional engineering helpers when `scale` is provided:
  - `field_name_scaled() -> f32` / `set_field_name_scaled(f32) -> Result<(), MbusError>`
- optional unit helper when `unit` is provided: `field_name_unit() -> &'static str`

`InputRegistersModel` generates:
- per-field getter/setter methods for local model updates
- `InputRegisterMap` implementation with FC04 `encode()` support
- no write trait methods (input registers are read-only from Modbus perspective)

`DiscreteInputsModel` generates:
- `DiscreteInputMap` implementation with FC02 `encode()` support
- read-only bit-packed encoding (LSB-first per Modbus spec)
- no write trait methods

### Ergonomic encode() calls

```rust
use mbus_server::prelude::*;
```

## Exception Handling

Every exception response the server sends triggers the `on_exception` callback on
`ModbusAppHandler`.  Override it to log, count, or react:

```rust
fn on_exception(
    &mut self,
    _txn_id: u16,
    _uid: UnitIdOrSlaveAddr,
    function_code: FunctionCode,
    exception_code: ExceptionCode,
    error: MbusError,
) {
    eprintln!("exception FC={function_code:?} code={exception_code:?} cause={error:?}");
}
```

## Write Hooks

`modbus_app` supports pre-write approval hooks for FC05/FC06/FC0F/FC10:

```rust
#[derive(Debug, Default)]
#[modbus_app(
    coils(coils, on_batch_write = on_coil_batch, on_write_0 = on_run_enable),
    holding_registers(regs, on_batch_write = on_reg_batch, on_write_10 = on_setpoint),
)]
struct App {
    coils: Coils,
    regs: Holding,
}
```

Hook signatures:
- single coil: `fn(&mut self, address: u16, old: bool, new: bool) -> Result<(), MbusError>`
- single register: `fn(&mut self, address: u16, old: u16, new: u16) -> Result<(), MbusError>`
- batch coil: `fn(&mut self, start: u16, qty: u16, values: &[u8]) -> Result<(), MbusError>`
- batch register: `fn(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError>`

Returning `Err(...)` rejects the write and leaves the model unchanged.

See `examples/write_hooks.rs` for a runnable end-to-end example.

## Forwarding Wrapper for Runtime App State

When your app model is wrapped by a mutex, RTOS primitive, or other container,
use `ForwardingApp<A>` to avoid writing repetitive delegation:

```rust
use std::sync::{Arc, Mutex};
use mbus_server::{ForwardingApp, ModbusAppAccess};

#[derive(Clone)]
struct SharedApp { inner: Arc<Mutex<MyApp>> }

impl ModbusAppAccess for SharedApp {
    type App = MyApp;
    fn with_app_mut<R, F: FnOnce(&mut MyApp) -> R>(&self, f: F) -> R {
        f(&mut self.inner.lock().expect("poisoned"))
    }
}

let app = ForwardingApp::new(shared_app);
// pass `app` to ServerServices::new(...)
```

## Broadcast Writes

Broadcast writes (Modbus slave address `0`) are Serial-only.  Enable them with:

```rust
ResilienceConfig { enable_broadcast_writes: true, ..Default::default() }
```

Supported FCs: 0x05, 0x0F, 0x06, 0x10.  The server **never** sends a response for
broadcast writes.  `unit_id_or_slave_addr.is_broadcast()` returns `true` in callbacks.

See `examples/broadcast_writes.rs`.

## Feature Flags

| Feature             | Default | Description                                                    |
|---------------------|---------|----------------------------------------------------------------|
| `coils`             | ✅       | FC 0x01, 0x05, 0x0F                                           |
| `holding-registers` | ✅       | FC 0x03, 0x06, 0x10, 0x16, 0x17                               |
| `input-registers`   | ✅       | FC 0x04                                                        |
| `discrete-inputs`   | ✅       | FC 0x02                                                        |
| `fifo`              | ✅       | FC 0x18                                                        |
| `file-record`       | ✅       | FC 0x14, 0x15                                                  |
| `diagnostics`       | ✅       | FC 0x07, 0x08, 0x0B, 0x0C, 0x11, 0x2B                        |
| `diagnostics-stats` | ❌       | Built-in FC08 counters; requires `diagnostics`                 |
| `traffic`           | ❌       | TX/RX traffic callbacks (`TrafficNotifier` trait)              |
| `logging`           | ❌       | `log` facade instrumentation                                   |
| `serial-ascii`      | ❌       | ASCII-mode ADU buffer sizing (`mbus-core` feature passthrough) |

### `diagnostics-stats`

When enabled the stack automatically tracks these counters, surfaced via FC08
sub-functions `0x000A`–`0x0014`:

- bus message count / comm error count / exception error count
- server message count / no-response count / NAK count / busy count
- bus character overrun count and flag

```toml
[dependencies]
mbus-server = { version = "0.2.0", features = ["diagnostics-stats"] }
```

## Examples

| Example                         | Function codes demonstrated              |
|---------------------------------|------------------------------------------|
| `full_featured_server`          | All 19 FCs in one app                     |
| `diagnostics`                   | FC07, FC11                               |
| `device_identification`         | FC2B / MEI 0x0E                          |
| `broadcast_writes`              | FC05, FC0F, FC06, FC10 (broadcast)       |
| `fifo_queue`                    | FC18                                     |
| `file_record`                   | FC14, FC15                               |
| `read_write_multiple_registers` | FC17                                     |
| `discrete_inputs_model`         | FC02 via `DiscreteInputsModel`           |
| `write_hooks`                   | FC05, FC0F, FC06, FC10 write hooks       |

## Attribute Keys Reference

**`CoilsModel`**: `#[coil(addr = N)]` — `addr` required, field type `bool`

**`HoldingRegistersModel`** / **`InputRegistersModel`**: `#[reg(addr = N, scale = …, unit = "…")]` —
`addr` required, field type `u16`, `scale` and `unit` optional

**`DiscreteInputsModel`**: `#[discrete_input(addr = N)]` — `addr` required, field type `bool`


## License

Licensed under the repository root `LICENSE`.
