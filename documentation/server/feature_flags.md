# Server Feature Flags

Control binary size and functionality by enabling only what you need.

---

## Quick Reference

| Feature | Default | Description |
|---------|---------|-------------|
| `server` | ❌ | Enables `mbus-server` runtime |
| `tcp` | ✅ | TCP transport (`StdTcpTransport`) |
| `serial-rtu` | ✅ | Serial RTU transport |
| `serial-ascii` | ❌ | Serial ASCII transport |
| `coils` | ✅ | FC01, FC05, FC0F |
| `holding-registers` | ✅ | FC03, FC06, FC10, FC16, FC17 |
| `input-registers` | ✅ | FC04 |
| `discrete-inputs` | ✅ | FC02 |
| `fifo` | ✅ | FC18 |
| `file-record` | ✅ | FC14, FC15 |
| `diagnostics` | ✅ | FC07, FC08, FC0B, FC0C, FC11, FC2B |
| `diagnostics-stats` | ❌ | Auto-handle FC08 counter sub-functions |
| `logging` | ❌ | `log` facade integration |

---

## Common Configurations

### Full Server (Everything)

```toml
[dependencies]
modbus-rs = { version = "0.6.0", features = ["server"] }
```

### Minimal TCP Server (Coils + Registers)

```toml
[dependencies]
modbus-rs = { version = "0.6.0", default-features = false, features = [
    "server",
    "tcp",
    "coils",
    "holding-registers"
] }
```

### Minimal Serial RTU Server

```toml
[dependencies]
modbus-rs = { version = "0.6.0", default-features = false, features = [
    "server",
    "serial-rtu",
    "coils",
    "holding-registers"
] }
```

### All Data Models, No Diagnostics

```toml
[dependencies]
modbus-rs = { version = "0.6.0", default-features = false, features = [
    "server",
    "tcp",
    "coils",
    "holding-registers",
    "input-registers",
    "discrete-inputs"
] }
```

---

## Feature Details

### `server`

Enables the `mbus-server` crate with `ServerServices`.

```rust
use modbus_rs::ServerServices;
```

---

### Function Code Features

#### `coils`

- FC01: Read Coils
- FC05: Write Single Coil
- FC0F: Write Multiple Coils

Callbacks: `read_coils_request`, `write_single_coil_request`, `write_multiple_coils_request`

#### `holding-registers`

- FC03: Read Holding Registers
- FC06: Write Single Register
- FC10: Write Multiple Registers
- FC16: Mask Write Register
- FC17: Read/Write Multiple Registers

Callbacks: `read_multiple_holding_registers_request`, `write_single_register_request`, `write_multiple_registers_request`, `mask_write_register_request`, `read_write_multiple_registers_request`

#### `input-registers`

- FC04: Read Input Registers

Callback: `read_input_registers_request`

#### `discrete-inputs`

- FC02: Read Discrete Inputs

Callback: `read_discrete_inputs_request`

#### `fifo`

- FC18: Read FIFO Queue

Callback: `read_fifo_queue_request`

#### `file-record`

- FC14: Read File Record
- FC15: Write File Record

Callbacks: `read_file_record_request`, `write_file_record_request`

---

### Diagnostics Features

#### `diagnostics`

Enables diagnostic function codes:

- FC07: Read Exception Status
- FC08: Diagnostics (with sub-functions)
- FC0B: Get Comm Event Counter
- FC0C: Get Comm Event Log
- FC11: Report Server ID
- FC2B: Read Device Identification (MEI 0x0E)

#### `diagnostics-stats`

Auto-handles FC08 counter sub-functions in the stack:

| Sub-function | Name |
|-------------|------|
| `0x000A` | Clear Counters |
| `0x000B` | Bus Message Count |
| `0x000C` | Bus Comm Error Count |
| `0x000D` | Bus Exception Error Count |
| `0x000E` | Server Message Count |
| `0x000F` | Server No-Response Count |
| `0x0010` | Server NAK Count |
| `0x0011` | Server Busy Count |
| `0x0012` | Bus Character Overrun Count |
| `0x0014` | Clear Overrun Counter/Flag |

---

### Optional Features

#### `logging`

Enables `log` facade calls for debugging:

```bash
RUST_LOG=debug cargo run --features server,logging
```

---

## Derive Macro Availability

The derive macros are always available when `server` is enabled:

| Macro | Requires |
|-------|----------|
| `CoilsModel` | `coils` |
| `HoldingRegistersModel` | `holding-registers` |
| `InputRegistersModel` | `input-registers` |
| `DiscreteInputsModel` | `discrete-inputs` |
| `modbus_app` | at least one data model feature |

---

## See Also

- [Building Applications](building_applications.md)
- [Function Codes](function_codes.md)
- [Macros](macros.md)
