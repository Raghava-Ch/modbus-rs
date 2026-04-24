# mbus-codegen

Code-generation library for the `mbus-ffi` server-app layer.

This crate exists to give `mbus-ffi/build.rs` and `xtask` a single shared source of truth
for server-app code generation so that the logic does not have to be duplicated across a
build script and a CLI tool.

It is primarily a support crate for the workspace rather than an end-user-facing library,
but it is publishable because `mbus-ffi` depends on it as a build-dependency.

---

## What It Does

Given a YAML device config that declares the Modbus register/coil map, this crate:

1. **Parses** the YAML into a typed `ServerAppConfig` struct (`parse_yaml`).
2. **Validates** the config — checks schema version, duplicate addresses, and Modbus
   read-only constraints on discrete inputs and input registers (`validate_config`).
3. **Renders** a Rust dispatcher source file (`render_rust_dispatcher`) that is
   compiled into `mbus-ffi` as the server-app layer.
4. **Renders** a C header (`render_c_header`) that exposes the generated API to
   the host C/C++ application.

---

## YAML Config Format

```yaml
schema_version: 1

device:
  name: PumpController
  unit_id: 1
  response_timeout_ms: 1000   # optional

hooks:
  on_write_coil: my_global_coil_hook     # optional: global fallback for coil writes
  on_write_holding: ~                    # optional: global fallback for holding-reg writes

memory_map:
  coils:
    - name: pump_run
      address: 0
      access: rw         # ro | wo | rw
      on_write: app_on_write_pump_run   # optional: per-entry hook, overrides global

  discrete_inputs:
    - name: fault_active
      address: 0
      access: ro         # must be ro — Modbus protocol constraint

  holding_registers:
    - name: speed_setpoint
      address: 0
      access: rw
      on_write: app_on_write_speed_setpoint

  input_registers:
    - name: pressure_actual
      address: 0
      access: ro         # must be ro — Modbus protocol constraint
```

**`access` values:**

| Value | Meaning |
|---|---|
| `ro` | Read-only from a Modbus client's perspective |
| `wo` | Write-only (no FC01/FC03/FC04 read support generated) |
| `rw` | Both readable and writable |

**Write-notification hooks (`on_write`):**

When set, the named C function is called **before** the value is stored in Rust state.
Return `MBUS_HOOK_OK` to accept the write, or another `MbusHookStatus` variant to
reject it (the Rust state is left unchanged and a Modbus exception is returned to the
client).

Per-entry `on_write` takes priority over the global `hooks.on_write_coil` /
`hooks.on_write_holding` fallbacks.

---

## Generated Outputs

### Rust dispatcher (`render_rust_dispatcher`)

Written to `$OUT_DIR/generated_server.rs` by `mbus-ffi/build.rs` at compile time.
Included via `include!` macro — never tracked in git.

Contains:
- `MbusHookStatus` enum (also `#[repr(C)]` for C linkage)
- `AppCoils`, `AppDiscreteInputs`, `AppHolding`, `AppInput` model structs derived
  with `mbus-server` proc-macros (`CoilsModel`, `HoldingRegistersModel`, …)
- `AppModel` struct annotated with `#[modbus_app]` — wires all models together
- `static mut APP_MODEL: Option<AppModel>` — Rust-owned register state
- Address-based `mbus_server_get_*/set_*` FFI functions for each table
- Named `mbus_server_get_{name}`/`set_{name}` FFI functions per entry
- Write-dispatch helpers that call the registered `on_write` hooks
- Handler callbacks matching the `MbusServerHandlers` signature table
- `mbus_server_model_init()` and `mbus_server_default_handlers(userdata)` convenience exports

### C header (`render_c_header`)

Written to a path specified via `--emit-c-header` when running `gen-server-app`.
Checked into the repo alongside the YAML — regenerate with:

```bash
cargo run -p xtask -- gen-server-app \
  --config path/to/server_app.yaml \
  --emit-c-header path/to/mbus_server_app.h
```

Contains:
- `MbusHookStatus` C enum
- `mbus_app_lock()` / `mbus_app_unlock()` declarations
- Write-notification hook declarations (one per `on_write` entry)
- Address-based accessor declarations
- `mbus_server_model_init()` / `mbus_server_default_handlers()` declarations
- Named field accessor declarations
- Generated handler callback declarations (for manual `MbusServerHandlers` assembly)

---

## How It Is Used

### By `mbus-ffi/build.rs` (Rust dispatcher)

`mbus-ffi/build.rs` reads `MBUS_SERVER_APP_CONFIG` (a path to the YAML config),
calls `parse_yaml` + `validate_config` + `render_rust_dispatcher`, and writes the
result to `$OUT_DIR/generated_server.rs`.

```bash
MBUS_SERVER_APP_CONFIG=path/to/server_app.yaml \
  cargo build -p mbus-ffi --features c-server
```

### By `xtask gen-server-app` (C header)

`xtask` uses the same types and `render_c_header` to regenerate the C header
whenever the YAML changes:

```bash
cargo run -p xtask -- gen-server-app \
  --config path/to/server_app.yaml \
  --emit-c-header path/to/mbus_server_app.h
```

`--check` verifies the file is up to date without writing; `--dry-run` prints
what would be written without touching the filesystem.

### By `xtask build-c-demo` (end-to-end)

`build-c-demo` reads `codegen` from `demo.yaml`, runs `gen-server-app` for the
C header, then sets `MBUS_SERVER_APP_CONFIG` when invoking `cargo build` so
`build.rs` generates the Rust dispatcher automatically:

```bash
cargo run -p xtask -- build-c-demo c_server_demo_yaml --static
```

---

## Public API Summary

```rust
// Parse YAML text into a typed config.
pub fn parse_yaml(text: &str) -> Result<ServerAppConfig, String>;

// Validate a parsed config (schema version, address uniqueness, ro constraints).
pub fn validate_config(config: &ServerAppConfig) -> Result<(), String>;

// Render the Rust dispatcher source (goes to $OUT_DIR/generated_server.rs).
pub fn render_rust_dispatcher(config: &ServerAppConfig) -> String;

// Render the C header source (goes to target/mbus-ffi/include/mbus_server_app.h).
pub fn render_c_header(config: &ServerAppConfig) -> String;
```

Config types re-exported for use by `xtask`:
`ServerAppConfig`, `DeviceConfig`, `HooksConfig`, `MemoryMap`, `MapEntry`, `Access`.
