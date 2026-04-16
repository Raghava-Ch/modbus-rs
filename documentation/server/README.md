# Server Documentation

This section covers everything you need to build Modbus server applications with `modbus-rs`.

---

## Quick Links

| Getting Started | Building | Reference |
|-----------------|----------|-----------|
| [Quick Start](quick_start.md) | [Building Applications](building_applications.md) | [Architecture](architecture.md) |
| [Examples](examples.md) | [Feature Flags](feature_flags.md) | [Policies](policies.md) |

---

## Core Concepts

| Concept | Documentation |
|---------|---------------|
| **Data Models** | [Building Applications](building_applications.md#data-models) |
| **Write Hooks** | [Write Hooks](write_hooks.md) |
| **Derive Macros** | [Macros](macros.md) |
| **Function Codes** | [Function Codes](function_codes.md) |

---

## Supported Transports

| Transport | Feature Flag | Documentation |
|-----------|--------------|---------------|
| Modbus TCP | `tcp` | [Building Applications](building_applications.md#tcp-transport) |
| Serial RTU | `serial-rtu` | [Building Applications](building_applications.md#serial-rtu-transport) |
| Serial ASCII | `serial-ascii` | [Building Applications](building_applications.md#serial-ascii-transport) |

---

## Supported Function Codes

| FC | Name | Feature Flag | Direction |
|----|------|--------------|-----------|
| `0x01` | Read Coils | `coils` | Read |
| `0x02` | Read Discrete Inputs | `discrete-inputs` | Read |
| `0x03` | Read Holding Registers | `holding-registers` | Read |
| `0x04` | Read Input Registers | `input-registers` | Read |
| `0x05` | Write Single Coil | `coils` | Write |
| `0x06` | Write Single Register | `holding-registers` | Write |
| `0x0F` | Write Multiple Coils | `coils` | Write |
| `0x10` | Write Multiple Registers | `holding-registers` | Write |
| `0x14` | Read File Record | `file-record` | Read |
| `0x15` | Write File Record | `file-record` | Write |
| `0x16` | Mask Write Register | `holding-registers` | Write |
| `0x17` | Read/Write Multiple Registers | `holding-registers` | R/W |
| `0x18` | Read FIFO Queue | `fifo` | Read |
| `0x2B` | Read Device Identification | `diagnostics` | Read |

See [Function Codes Reference](function_codes.md) for complete details.

---

## Document Index

### Getting Started

- **[Quick Start](quick_start.md)** — First server in 5 minutes
- **[Examples Reference](examples.md)** — All examples with run commands

### Development Guides

- **[Building Applications](building_applications.md)** — Complete guide to building server apps
- **[Write Hooks](write_hooks.md)** — React to client writes
- **[Macros](macros.md)** — Derive macros for data models

### Reference

- **[Feature Flags](feature_flags.md)** — Enable only what you need
- **[Architecture](architecture.md)** — Internal design
- **[Policies](policies.md)** — Timeouts, retry queues, overflow handling
- **[Function Codes](function_codes.md)** — Complete FC reference

---

## Next Steps

1. Start with [Quick Start](quick_start.md) to run your first server
2. Review [Examples](examples.md) for your use case
3. Read [Building Applications](building_applications.md) for production setup
