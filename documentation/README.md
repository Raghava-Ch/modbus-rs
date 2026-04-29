# modbus-rs Documentation

Welcome to the modbus-rs documentation. This guide covers both client and server development across all supported transports and environments.

---

## Quick Navigation

| I want to... | Go to |
|--------------|-------|
| Build a Modbus **client** application | [Client Documentation](client/README.md) |
| Build a Modbus **server** application | [Server Documentation](server/README.md) |
| Build with **Python bindings** | [Python Bindings](python_bindings.md) |
| Migrate from an older version | [Migration Guide](migration_guide.md) |

---

## Client Documentation

Everything you need to build Modbus client applications.

| Document | Description |
|----------|-------------|
| [Client Overview](client/README.md) | Introduction and navigation |
| [Quick Start](client/quick_start.md) | Get your first client running in minutes |
| [Examples Reference](client/examples.md) | All examples with descriptions and run commands |
| [Building Applications](client/building_applications.md) | Callbacks, poll loop, transport setup |
| [Feature Flags](client/feature_flags.md) | Enable only what you need |
| [Architecture](client/architecture.md) | State machine, transport layer, services |
| [Policies](client/policies.md) | Retry, backoff, jitter, timeout configuration |
| [Async Development](client/async.md) | Tokio-based async client APIs |
| [C/FFI Development](client/c_bindings.md) | Native C client bindings |
| [WASM Development](client/wasm.md) | Browser WebSocket client |
| [Python Bindings](python_bindings.md) | Python client/server usage via mbus-ffi |

---

## Server Documentation

Everything you need to build Modbus server applications.

| Document | Description |
|----------|-------------|
| [Server Overview](server/README.md) | Introduction and navigation |
| [Quick Start](server/quick_start.md) | Get your first server running in minutes |
| [Examples Reference](server/examples.md) | All examples with descriptions and run commands |
| [Building Applications](server/building_applications.md) | ModbusAppHandler, transports, poll loop |
| [Feature Flags](server/feature_flags.md) | Enable only what you need |
| [Architecture](server/architecture.md) | Request dispatch, queuing, resilience |
| [Policies](server/policies.md) | Timeout, overflow, broadcast, priority queue |
| [Function Codes](server/function_codes.md) | Supported FCs with callback mapping |
| [Derive Macros](server/macros.md) | CoilsModel, HoldingRegistersModel, modbus_app |
| [Write Hooks](server/write_hooks.md) | Pre-write approval and validation |

---

## Gateway Documentation

Bridge upstream Modbus clients to downstream Modbus devices across different
transports (TCP ↔ RTU, WebSocket ↔ TCP, etc.).

| Document | Description |
|----------|-------------|
| [Quick Start](gateway/quick_start.md) | Sync, async TCP, and WebSocket gateway in minutes |
| [WebSocket Gateway](gateway/ws_gateway.md) | `AsyncWsGatewayServer` — WASM client → raw TCP/RTU bridge |
| [Architecture](gateway/architecture.md) | Request flow, transaction remapping, session model |
| [Routing](gateway/routing.md) | `UnitRouteTable`, `RangeRouteTable`, custom policies |
| [Feature Flags](gateway/feature_flags.md) | `async`, `ws-server`, `serial-rtu`, no_std usage |
| [no_std Usage](gateway/no_std.md) | Embedded / bare-metal sync gateway core |

---

## Cross-Reference

### Workspace Crates

| Crate | Purpose | Docs |
|-------|---------|------|
| [`modbus-rs`](../modbus-rs/README.md) | Top-level facade crate | [Client](client/README.md) / [Server](server/README.md) |
| [`mbus-client`](../mbus-client/README.md) | Client state machine | [Client Docs](client/README.md) |
| [`mbus-server`](../mbus-server/README.md) | Server runtime | [Server Docs](server/README.md) |
| [`mbus-core`](../mbus-core/README.md) | Protocol types, transport trait | Shared |
| [`mbus-async`](../mbus-async/README.md) | Async facade | [Client Async](client/async.md) |
| [`mbus-network`](../mbus-network/README.md) | TCP transport | [Client](client/building_applications.md) / [Server](server/building_applications.md) |
| [`mbus-serial`](../mbus-serial/README.md) | Serial RTU/ASCII transports | [Client](client/building_applications.md) / [Server](server/building_applications.md) |
| [`mbus-ffi`](../mbus-ffi/README.md) | C, WASM, and Python bindings | [C Bindings](client/c_bindings.md) / [WASM](client/wasm.md) / [Python](python_bindings.md) |

### Additional Resources

- [Contributing Guide](../CONTRIBUTING.md) — How to contribute
- [Release Checklist](../RELEASE.md) — Release process
- [License](../LICENSE) — GPLv3

---

## Getting Help

- **GitHub Issues**: [modbus-rs/issues](https://github.com/Raghava-Ch/modbus-rs/issues)
- **Email**: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)
