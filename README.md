# modbus-rs

A cross-platform, low-footprint Modbus client and server library for Rust.

- **no_std compatible** — runs on embedded MCUs and standard OS targets
- **All transports** — TCP, Serial RTU, Serial ASCII
- **Sync and async** — poll-driven core with optional Tokio facade
- **Feature-gated** — enable only what you need for minimal binary size
- **C and WASM bindings** — native C/C++ and browser integration via `mbus-ffi`

---

## Quick Start

```toml
[dependencies]
modbus-rs = "0.5.0"
```

```rust
use modbus_rs::{ClientServices, ModbusConfig, ModbusTcpConfig, StdTcpTransport};

let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
let mut client = ClientServices::<_, _, 4>::new(StdTcpTransport::new(), app, config)?;
client.connect()?;
client.coils().read_coils(1, unit_id, 0, 16)?;
loop { client.poll(); }
```

📖 **[Full Documentation →](documentation/README.md)**

---

## Documentation

| Section | Quick Links |
|---------|-------------|
| **Client** | [Quick Start](documentation/client/quick_start.md) · [Examples](documentation/client/examples.md) · [Building Apps](documentation/client/building_applications.md) · [Async](documentation/client/async.md) |
| **Server** | [Quick Start](documentation/server/quick_start.md) · [Examples](documentation/server/examples.md) · [Macros](documentation/server/macros.md) · [Write Hooks](documentation/server/write_hooks.md) |
| **Bindings** | [C/FFI](documentation/client/c_bindings.md) · [WASM](documentation/client/wasm.md) |
| **Reference** | [Feature Flags](documentation/client/feature_flags.md) · [Migration Guide](documentation/migration_guide.md) |

---

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| [`modbus-rs`](modbus-rs/) | Top-level convenience crate — start here |
| [`mbus-client`](mbus-client/) | Client state machine and request services |
| [`mbus-server`](mbus-server/) | Server runtime with derive macros |
| [`mbus-core`](mbus-core/) | Shared protocol types and transport trait |
| [`mbus-async`](mbus-async/) | Tokio async facade |
| [`mbus-network`](mbus-network/) | TCP transport implementation |
| [`mbus-serial`](mbus-serial/) | Serial RTU/ASCII transport implementation |
| [`mbus-ffi`](mbus-ffi/) | Native C and WASM bindings |

---

## Feature Flags

| Flag | Description |
|------|-------------|
| `client` | Client state machine (default) |
| `server` | Server runtime and macros |
| `tcp` | Modbus TCP transport (default) |
| `serial-rtu` | Serial RTU transport (default) |
| `serial-ascii` | Serial ASCII transport |
| `async` | Tokio async facade |
| `coils` | FC01, FC05, FC0F (default) |
| `registers` | FC03, FC04, FC06, FC10 (default) |
| `discrete-inputs` | FC02 (default) |
| `diagnostics` | FC07, FC08, FC2B, etc. (default) |
| `traffic` | Raw TX/RX frame callbacks |
| `logging` | `log` facade integration |

See [Feature Flags Reference](documentation/client/feature_flags.md) for complete details.

---

## Examples

### TCP Client

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 192.168.1.10 502 1
```

### Serial RTU Client

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers -- /dev/ttyUSB0 1
```

### Async Client

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --features async
```

### TCP Server

```bash
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo --features server
```

📖 **[All Examples →](documentation/client/examples.md)**

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and contribution workflow.

## License

This project is licensed under the **GNU General Public License v3.0 (GPLv3)** — see [LICENSE](LICENSE).

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

---

**Repository:** [github.com/Raghava-Ch/modbus-rs](https://github.com/Raghava-Ch/modbus-rs)