# Examples Layout

Examples are grouped by core crate name and role to make client/server/core scenarios easy to differentiate.

## Directory Groups

- `examples/modbus-rs/client/`:
  - Client-facing runnable examples for the `modbus-rs` crate.
- `examples/mbus-server/server/`:
  - Reserved for server-oriented examples (when added in this crate/workspace).
- `examples/mbus-core/core/`:
  - Reserved for core data-model/protocol examples.

## Example Naming Convention

Use namespaced example target names in `Cargo.toml`:

- `modbus_rs_client_*` for client examples
- `modbus_rs_server_*` for server examples
- `mbus_core_*` for core examples

This avoids collisions like `coils_example` between client and server samples and keeps `cargo run --example ...` self-descriptive.

## Current Registered Example Targets

Current `modbus-rs` package targets are client-focused and use `modbus_rs_client_*` names.
See `modbus-rs/Cargo.toml` `[[example]]` entries for the canonical list.

### Real-world HVAC Server (mbus-network server transport)

This example is a server-only Modbus TCP application that uses `AcceptedTcpTransport` from `mbus-network` with `mbus-server` macros (`HoldingRegistersModel`, `CoilsModel`, `modbus_app`).

Run:

```bash
cargo run -p modbus-rs --example modbus_rs_server_std_transport_client_demo
```

Optional args:

```bash
cargo run -p modbus-rs --example modbus_rs_server_std_transport_client_demo -- --host 0.0.0.0 --port 5502 --unit 1
```

Optional env:

- `MBUS_SERVER_HOST` (default `0.0.0.0`)
- `MBUS_SERVER_PORT` (default `5502`)
- `MBUS_SERVER_UNIT` (default `1`)

Delegation model used by server examples:

- `#[modbus_app(...)]` implements `ModbusAppHandler` for the concrete app model.
- Runtime wrappers implement `ModbusAppAccess` once.
- `ForwardingApp::new(wrapper)` adapts the wrapper to `ServerServices`.

This avoids writing manual per-function-code delegation methods and works with:

- `std` synchronization (`Arc<Mutex<_>>`) for shared state across threads.
- lock-free per-worker ownership (`RefCell<_>` pattern) where each worker owns its own app.
- RTOS or bare-metal synchronization primitives in custom wrappers.
