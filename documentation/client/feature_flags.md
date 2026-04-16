# Client Feature Flags

Control binary size and functionality by enabling only what you need.

---

## Quick Reference

| Feature | std default | no-std default | Requires std | Description |
|---------|:-----------:|:--------------:|:---:|-------------|
| `client` | ✅ | ✅ | — | `mbus-client` state machine (no_std compatible) |
| `tcp` | ✅ | ❌ | ✅ | TCP transport (`StdTcpTransport`) |
| `serial-rtu` | ✅ | ❌ | ✅ | Serial RTU transport (`StdRtuTransport`) |
| `serial-ascii` | ❌ | ❌ | ✅ | Serial ASCII transport (`StdAsciiTransport`) |
| `async` | ❌ | ❌ | ✅ | Tokio async facade (`AsyncTcpClient`, `AsyncSerialClient`) |
| `coils` | ✅ | ✅ | — | FC01, FC05, FC0F |
| `registers` | ✅ | ✅ | — | FC03, FC04, FC06, FC10 |
| `discrete-inputs` | ✅ | ✅ | — | FC02 |
| `fifo` | ✅ | ✅ | — | FC18 |
| `file-record` | ✅ | ✅ | — | FC14, FC15 |
| `diagnostics` | ✅ | ✅ | — | FC07, FC08, FC0B, FC0C, FC11, FC2B |
| `traffic` | ❌ | ❌ | — | Raw TX/RX frame callbacks |
| `logging` | ❌ | ❌ | ✅ | `log` facade integration |
| `no-std` | ❌ | —  | — | Convenience bundle: `client` + all FC models, no transports |

---

## Common Configurations

### Full Default (Everything)

```toml
[dependencies]
modbus-rs = "0.5.0"
```

Includes: `client`, `tcp`, `serial-rtu`, all function codes. Requires std.

---

### Embedded / no_std

For targets without std (bare-metal MCUs, RTOS). You bring your own `Transport` implementation.

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = ["no-std"] }
```

Includes: `client` state machine + all function code models (`coils`, `registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`). No transport, no OS.

Pickup only the FC models you need to keep code size minimal:

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "coils",
    "registers",
] }
```

---

### Minimal TCP Client

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "tcp",
    "coils"
] }
```

Binary size: ~50% smaller than full default.

---

### Minimal Serial RTU Client

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "serial-rtu",
    "registers"
] }
```

---

### TCP + Serial RTU (No ASCII)

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "tcp",
    "serial-rtu",
    "coils",
    "registers"
] }
```

---

### Async TCP Client

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "async",
    "tcp",
    "coils",
    "registers"
] }
tokio = { version = "1", features = ["full"] }
```

---

### With Traffic Observability

```toml
[dependencies]
modbus-rs = { version = "0.5.0", features = ["traffic"] }
```

---

### With Logging

```toml
[dependencies]
modbus-rs = { version = "0.5.0", features = ["logging"] }
env_logger = "0.11"
```

Then in your code:

```rust
fn main() {
    env_logger::init();
    // RUST_LOG=debug cargo run ...
}
```

---

## Feature Details

### Transport Features

> **Requires std.** Transport features (`tcp`, `serial-rtu`, `serial-ascii`, `async`) depend on OS primitives and are not available on `no_std` targets. Use `default-features = false` and omit them for embedded builds.

#### `tcp`

Enables `StdTcpTransport` using `std::net::TcpStream`.

#### `serial-rtu`

Enables `StdRtuTransport` for Modbus RTU over serial.

#### `serial-ascii`

Enables `StdAsciiTransport` for Modbus ASCII over serial.

**Note:** ASCII mode increases `MAX_ADU_FRAME_LEN` from 260 to 513 bytes.

---

### Function Code Features

#### `coils`

- FC01: Read Coils
- FC05: Write Single Coil
- FC0F: Write Multiple Coils

#### `registers`

- FC03: Read Holding Registers
- FC04: Read Input Registers
- FC06: Write Single Register
- FC10: Write Multiple Registers

#### `discrete-inputs`

- FC02: Read Discrete Inputs

#### `fifo`

- FC18: Read FIFO Queue

#### `file-record`

- FC14: Read File Record
- FC15: Write File Record

#### `diagnostics`

- FC07: Read Exception Status
- FC08: Diagnostics
- FC0B: Get Comm Event Counter
- FC0C: Get Comm Event Log
- FC11: Report Server ID
- FC2B: Read Device Identification

---

### Optional Features

#### `async`

Enables Tokio-based async clients:

- `AsyncTcpClient`
- `AsyncSerialClient`

See [Async Development](async.md).

#### `traffic`

Enables `TrafficNotifier` trait for raw frame observability.

```rust
impl TrafficNotifier for App {
    fn on_tx_frame(&self, txn_id: u16, uid: UnitIdOrSlaveAddr, frame: &[u8]) { }
    fn on_rx_frame(&self, txn_id: u16, uid: UnitIdOrSlaveAddr, frame: &[u8]) { }
    // ...
}
```

#### `logging`

Enables `log` facade calls in transport layers.

```bash
RUST_LOG=debug cargo run --features logging
```

---

## Embedded / no_std Considerations

The core library is `no_std` compatible. For embedded targets:

1. Use `default-features = false`
2. Enable only function codes you need
3. Provide your own `Transport` implementation if not using std transports
4. Implement `TimeKeeper` with your hardware timer

```toml
[dependencies]
modbus-rs = { version = "0.5.0", default-features = false, features = [
    "client",
    "coils"
] }
```

---

## See Also

- [Building Applications](building_applications.md)
- [Architecture](architecture.md)
- [Async Development](async.md)
