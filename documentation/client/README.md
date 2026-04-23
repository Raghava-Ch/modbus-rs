# Client Documentation

This section covers everything you need to build Modbus client applications with `modbus-rs`.

---

## Quick Links

| Getting Started | Building | Reference |
|-----------------|----------|-----------|
| [Quick Start](quick_start.md) | [Building Applications](building_applications.md) | [Architecture](architecture.md) |
| [Examples](examples.md) | [Feature Flags](feature_flags.md) | [Policies](policies.md) |

---

## Development Environments

| Environment | Documentation |
|-------------|---------------|
| **Sync Rust** (poll-driven) | [Building Applications](building_applications.md) |
| **Async Rust** (Tokio) | [Async Development](async.md) |
| **C/C++ Native** | [C/FFI Bindings](c_bindings.md) |
| **Browser/WASM** (WebSocket + Web Serial) | [WASM Development](wasm.md) |

---

## Supported Transports

| Transport | Feature Flag | Documentation |
|-----------|--------------|---------------|
| Modbus TCP | `network-tcp` | [Building Applications](building_applications.md#tcp-transport) |
| Serial RTU | `serial-rtu` | [Building Applications](building_applications.md#serial-rtu-transport) |
| Serial ASCII | `serial-ascii` | [Building Applications](building_applications.md#serial-ascii-transport) |

---

## Supported Function Codes

| FC | Name | Feature Flag |
|----|------|--------------|
| `0x01` | Read Coils | `coils` |
| `0x02` | Read Discrete Inputs | `discrete-inputs` |
| `0x03` | Read Holding Registers | `registers` |
| `0x04` | Read Input Registers | `registers` |
| `0x05` | Write Single Coil | `coils` |
| `0x06` | Write Single Register | `registers` |
| `0x0F` | Write Multiple Coils | `coils` |
| `0x10` | Write Multiple Registers | `registers` |
| `0x16` | Mask Write Register | `registers` |
| `0x17` | Read/Write Multiple Registers | `registers` |
| `0x18` | Read FIFO Queue | `fifo` |
| `0x14` | Read File Record | `file-record` |
| `0x15` | Write File Record | `file-record` |
| `0x07` | Read Exception Status | `diagnostics` |
| `0x08` | Diagnostics | `diagnostics` |
| `0x0B` | Get Comm Event Counter | `diagnostics` |
| `0x0C` | Get Comm Event Log | `diagnostics` |
| `0x11` | Report Server ID | `diagnostics` |
| `0x2B/0x0E` | Read Device Identification (MEI) | `diagnostics` |
| `0x2B` | Encapsulated Interface Transport (MEI) | `diagnostics` |

---

## Document Index

### Getting Started

- **[Quick Start](quick_start.md)** — First client in 5 minutes
- **[Examples Reference](examples.md)** — All examples with run commands

### Development Guides

- **[Building Applications](building_applications.md)** — Complete guide to building client apps
- **[Async Development](async.md)** — Tokio-based async client APIs
- **[C/FFI Bindings](c_bindings.md)** — Native C client integration
- **[WASM Development](wasm.md)** — Browser WebSocket client

### Reference

- **[Feature Flags](feature_flags.md)** — Enable only what you need
- **[Architecture](architecture.md)** — State machine, services, transport
- **[Policies](policies.md)** — Retry, backoff, jitter, timeout

---

## Next Steps

1. Start with [Quick Start](quick_start.md) to run your first client
2. Review [Examples](examples.md) for your use case
3. Read [Building Applications](building_applications.md) for production setup
