# Server Feature Flags

Feature flags that matter when building server applications from the top-level `modbus-rs` crate.

---

## Quick Reference

The `Default` column below refers to the top-level `modbus-rs` crate with default features enabled.

| Feature | Default | Description |
|---------|---------|-------------|
| `server` | ✅ | Enables `mbus-server` and the synchronous server runtime |
| `network-tcp` | ✅ | Enables TCP transports |
| `serial-rtu` | ✅ | Enables Serial RTU transports |
| `serial-ascii` | ❌ | Enables Serial ASCII framing and transport support |
| `coils` | ✅ | FC01, FC05, FC0F |
| `holding-registers` | ✅ | FC03, FC06, FC10, FC16, FC17 |
| `input-registers` | ✅ | FC04 |
| `discrete-inputs` | ✅ | FC02 |
| `fifo` | ✅ | FC18 |
| `file-record` | ✅ | FC14, FC15 |
| `diagnostics` | ✅ | FC07, FC08, FC0B, FC0C, FC11, FC2B/0x0E |
| `diagnostics-stats` | ❌ | Stack-managed FC08 counter sub-functions |
| `logging` | ❌ | Logging integration for transport/client crates |
| `async` | ❌ | Pulls in `mbus-async` |
| `traffic` | ❌ | Traffic notifier support |

If you use `default-features = false`, none of the above are enabled until you list them.

---

## Common Configurations

### Keep the Full Default Stack

```toml
[dependencies]
modbus-rs = "0.7.0"
```

This includes client and server support.

### TCP Server Only

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "server",
    "network-tcp",
    "coils",
    "holding-registers"
] }
```

### Serial RTU Server With Diagnostics

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "server",
    "serial-rtu",
    "coils",
    "holding-registers",
    "input-registers",
    "diagnostics"
] }
```

### Server With Automatic Diagnostic Counters

```toml
[dependencies]
modbus-rs = { version = "0.7.0", default-features = false, features = [
    "server",
    "network-tcp",
    "coils",
    "holding-registers",
    "diagnostics",
    "diagnostics-stats"
] }
```

---

## Notes By Feature

### `server`

Re-exports `ServerServices`, the split server traits, and the derive / routing macros.

### `network-tcp`

Enables the synchronous TCP transport and, if `async` is also enabled, the async TCP server adapter.

### `serial-rtu` and `serial-ascii`

Enable the synchronous serial transports and, when `async` is present, the async serial server adapter.

### `diagnostics`

Enables:

- FC07 Read Exception Status
- FC08 Diagnostics
- FC0B Get Comm Event Counter
- FC0C Get Comm Event Log
- FC11 Report Server ID
- FC2B / MEI 0x0E Read Device Identification

### `diagnostics-stats`

Builds on `diagnostics` and lets the stack answer the FC08 counter sub-functions directly.

### `traffic`

Enables the `TrafficNotifier` trait for observing raw RX/TX frames and framing or send errors.

---

## Derive Macros

The derive macros depend on the corresponding server-side model features:

| Macro | Requires |
|-------|----------|
| `CoilsModel` | `server` + `coils` |
| `HoldingRegistersModel` | `server` + `holding-registers` |
| `InputRegistersModel` | `server` + `input-registers` |
| `DiscreteInputsModel` | `server` + `discrete-inputs` |
| `modbus_app` | `server` plus at least one routed group |

---

## See Also

- [Quick Start](quick_start.md)
- [Function Codes](function_codes.md)
- [Macros](macros.md)
