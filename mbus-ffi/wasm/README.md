# modbus-rs-wasm

Browser-native WebAssembly bindings for [modbus-rs](https://github.com/Raghava-Ch/modbus-rs), enabling Modbus TCP (via WebSockets) and Modbus RTU/ASCII (via Web Serial) directly in the browser.

> **Note:** This package is designed for browser-native environments (using WebSockets and Web Serial). If you are building a Node.js backend application, use the native [`modbus-rs`](https://www.npmjs.com/package/modbus-rs) package instead. Running this package in pure Node.js requires `--experimental-wasm-modules` and is not officially supported.

## Installation

```bash
npm install modbus-rs-wasm
```

## Integration & Frameworks (Vite, Svelte, React, etc.)

No custom resolver aliases or configuration workarounds are required starting in `v0.14.0`. Standard package entry points are resolved automatically based on your builder/bundler targets.

### Web (Direct / HTML / Vanilla JS)
When loading the package in browser environments without a bundler, import the web entry point and await the initialization promise:

```javascript
import init, { WasmModbusClient } from 'modbus-rs-wasm/dist/web/modbus-rs.js';

await init();
const client = new WasmModbusClient('ws://localhost:8502', 1, 5000, 3, 20);
```

### Bundlers & Frameworks (Vite, SvelteKit, Next.js, etc.)
When using modern bundlers, the root import automatically maps to the bundler target:

```javascript
import { WasmModbusClient } from 'modbus-rs-wasm';
```
*(Make sure your bundler is configured to load WebAssembly, e.g., using `vite-plugin-wasm` and `vite-plugin-top-level-await` in Vite).*


## Quick Start (Modbus TCP via WebSocket Gateway)

First, run the gateway [modbus-gateway](https://github.com/Raghava-Ch/modbus-gateway).

```javascript
import { WasmModbusClient } from 'modbus-rs-wasm';

async function readRegisters() {
  // Connect to a WebSocket proxy that forwards to a Modbus TCP device
  // (e.g. ws_url, unit_id, response_timeout_ms, retries, tick_interval_ms)
  const client = new WasmModbusClient('ws://localhost:8502', 1, 5000, 3, 20);

  try {
    // Read 10 holding registers starting at address 0
    const registers = await client.read_holding_registers(0, 10);
    console.log('Holding registers:', registers); // Uint16Array
  } catch (error) {
    console.error('Failed to read registers:', error);
  }
}
```

## Quick Start (Modbus RTU via Web Serial)

*Web Serial requires a Chromium-based browser (Chrome, Edge, Opera) and must be initiated by a user gesture (e.g., button click).*

If you are using modbus-rs on Node.js, you can use the native [`modbus-rs`](https://www.npmjs.com/package/modbus-rs) package instead.

If you have limitation on serial port using browser, then you may be interested in connecting serial port over ws/tcp so you can use the gateway application [modbus-gateway](https://github.com/Raghava-Ch/modbus-gateway).

```javascript
import { request_serial_port, WasmSerialModbusClient } from 'modbus-rs-wasm';

document.getElementById('connect-btn').addEventListener('click', async () => {
  try {
    // 1. Prompt user to select a serial port
    const portHandle = await request_serial_port();
    
    // 2. Connect client (handle, unit_id, mode, baud, data_bits, stop_bits, parity, timeout, retries, tick)
    const client = new WasmSerialModbusClient(
      portHandle, 1, 'rtu', 19200, 8, 1, 'even', 1000, 3, 20
    );

    // 3. Read coils
    const coils = await client.read_coils(0, 8);
    console.log('Coils:', coils); // Uint8Array
    
  } catch (err) {
    console.error('Serial error:', err);
  }
});
```

## License

GPL-3.0-only — see [LICENSE](./LICENSE). A commercial license is available for proprietary use.
