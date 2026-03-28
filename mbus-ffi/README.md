# mbus-ffi

WASM/JS bindings for the modbus-rs stack.

## Position In Workspace

`mbus-ffi` is an implementation crate inside this workspace.

For browser/WASM application code, import and use `mbus-ffi` directly.
`modbus-rs` does not provide a top-level `wasm` feature and does not re-export
`mbus-ffi` WASM-facing types.

This crate exposes browser-friendly Modbus clients over:

- WebSocket (Modbus TCP gateway): `WasmModbusClient`
- Web Serial (RTU/ASCII): `request_serial_port`, `WasmSerialPortHandle`, `WasmSerialModbusClient`

All APIs are Promise-based and are designed for browser runtimes (`wasm32`).

## Status

Implemented and usable for browser integration and smoke testing.

## What This Crate Exports

When compiled for `wasm32`, `mbus-ffi` exports:

- `WasmModbusClient` (WebSocket transport)
- `request_serial_port()`
- `WasmSerialPortHandle`
- `WasmSerialModbusClient` (Web Serial transport)

These symbols are conditionally compiled behind `target_arch = "wasm32"` so native builds are unaffected.

## Feature Flags

`mbus-ffi` uses modular feature flags:

- `wasm`: enables WASM bindings and browser transports (`mbus-network/wasm`, `mbus-serial/wasm`)
- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- `full`: enables all Modbus service features above

Typical web builds use `--features wasm,full`.

## Build WASM Package

From `mbus-ffi`:

```bash
wasm-pack build --target web --features wasm,full
```

Generated JS/WASM package is written to `mbus-ffi/pkg`.

## Quick Start (WebSocket)

```javascript
import init, { WasmModbusClient } from "./pkg/mbus_ffi.js";

await init();

const client = new WasmModbusClient(
	"ws://127.0.0.1:8080", // ws_url
	1,                      // unit_id
	3000,                   // response_timeout_ms
	1,                      // retry_attempts
	20                      // tick_interval_ms
);

const regs = await client.read_holding_registers(0, 2);
console.log(Array.from(regs));
```

## Quick Start (Web Serial)

```javascript
import init, { request_serial_port, WasmSerialModbusClient } from "./pkg/mbus_ffi.js";

await init();

// Must be called from a user gesture (e.g. button click)
const portHandle = await request_serial_port();

const client = new WasmSerialModbusClient(
	portHandle,
	1,      // unit_id
	"rtu", // mode: "rtu" | "ascii"
	9600,   // baud_rate
	8,      // data_bits
	1,      // stop_bits
	"even",// parity: "none" | "even" | "odd"
	3000,   // response_timeout_ms
	1,      // retry_attempts
	20      // tick_interval_ms
);

const ok = await client.read_single_coil(0);
console.log(ok);
```

## Supported Modbus Operations

Both WASM clients expose the same service surface:

- Coils: read single/multiple, write single/multiple
- Registers: read holding/input, write single/multiple, mask write, read-write multiple
- Discrete inputs: read single/multiple
- FIFO queue: read
- File record: read/write
- Diagnostics: exception status, diagnostics, comm event counter/log, report server id, read device identification

## Example Smoke Pages

Use the browser examples under `mbus-ffi/examples`:

- `network_smoke.html` (WebSocket/TCP path)
- `serial_smoke.html` (Web Serial path, full serial API smoke runner)

Serve the examples over localhost after building `pkg`, for example:

```bash
cd mbus-ffi
python3 -m http.server 8089
```

Then open:

- `http://localhost:8089/examples/network_smoke.html`
- `http://localhost:8089/examples/serial_smoke.html`

## Running WASM Tests

The E2E WASM tests live in `mbus-ffi/tests/wasm_e2e.rs` and run in browser mode.

Run the full browser feature test suite:

```bash
cd mbus-ffi;
wasm-pack test --chrome --target wasm32-unknown-unknown --features wasm,full
```

Fast compile check:

```bash
cd mbus-ffi
cargo check --target wasm32-unknown-unknown --features wasm,full --tests
```

Run browser tests (Chrome headless):

```bash
cd mbus-ffi
wasm-pack test --chrome --headless --features wasm,full
```

## Browser Requirements (Serial)

Web Serial requires:

- Chromium-based browser
- Secure context (HTTPS) or localhost
- User gesture for `request_serial_port()`

## Notes

- Promise rejection errors are surfaced as stringified internal errors.
- `WasmSerialModbusClient` uses serial-safe pipeline behavior internally.
- Native (non-wasm32) consumers should use the core Rust crates directly.

## License

Licensed under the repository root `LICENSE`.
