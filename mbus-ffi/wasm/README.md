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
import init, { WasmTcpTransport } from 'modbus-rs-wasm/dist/web/modbus-rs.js';

await init();
const transport = new WasmTcpTransport('ws://localhost:8502', { 
  responseTimeoutMs: 5000, 
  retryAttempts: 3, 
  tickIntervalMs: 20 
});
const client = transport.create_client({ unitId: 1 });
```

### Bundlers & Frameworks (Vite, SvelteKit, Next.js, etc.)
When using modern bundlers, the root import automatically maps to the bundler target:

```javascript
import { WasmTcpTransport } from 'modbus-rs-wasm';
```
*(Make sure your bundler is configured to load WebAssembly, e.g., using `vite-plugin-wasm` and `vite-plugin-top-level-await` in Vite).*

## Quick Start

### Examples
Ready-to-run HTML examples demonstrating both Modbus client and server functionality in the browser:

- **[wasm_client/network_smoke.html](./examples/wasm_client/network_smoke.html)**: WebSocket Modbus TCP client example.
- **[wasm_client/serial_smoke.html](./examples/wasm_client/serial_smoke.html)**: Web Serial Modbus RTU client example.
- **[wasm_server/network_smoke.html](./examples/wasm_server/network_smoke.html)**: WebSocket Modbus TCP server example.
- **[wasm_server/serial_smoke.html](./examples/wasm_server/serial_smoke.html)**: Web Serial Modbus RTU server example.

To run the examples locally:
```bash
npx serve examples/
```

### Modbus RTU via Web Serial

*Web Serial requires a Chromium-based browser (Chrome, Edge, Opera) and must be initiated by a user gesture (e.g., button click).*

If you are using modbus-rs on Node.js, you can use the native [`modbus-rs`](https://www.npmjs.com/package/modbus-rs) package instead.

If you have limitation on serial port using browser, then you may be interested in connecting serial port over ws/tcp so you can use the gateway application [modbus-gateway](https://github.com/Raghava-Ch/modbus-gateway).

```javascript
import { request_serial_port, WasmSerialTransport } from 'modbus-rs-wasm';

document.getElementById('connect-btn').addEventListener('click', async () => {
  try {
    // 1. Prompt user to select a serial port
    const portHandle = await request_serial_port();
    
    // 2. Create RTU Transport
    const transport = new WasmSerialTransport(portHandle, {
      mode: 'rtu',
      baudRate: 19200,
      dataBits: 8,
      stopBits: 1,
      parity: 'even',
      responseTimeoutMs: 1000,
      retryAttempts: 3,
      tickIntervalMs: 20
    });

    // 3. Spawn a client for a specific slave (Unit ID)
    const client = transport.create_client({ unitId: 10 });

    // 4. Read coils
    const coils = await client.read_coils(0, 8);
    console.log('Coils:', coils); // Uint8Array
    
  } catch (err) {
    console.error('Serial error:', err);
  }
});
```

### Modbus TCP via WebSocket Gateway
First, run the gateway [modbus-gateway](https://github.com/Raghava-Ch/modbus-gateway).

```javascript
import { WasmTcpTransport } from 'modbus-rs-wasm';

async function readRegisters() {
  // 1. Connect to a WebSocket proxy that forwards to a Modbus TCP device
  const transport = new WasmTcpTransport('ws://localhost:8502', { 
    responseTimeoutMs: 5000, 
    retryAttempts: 3, 
    tickIntervalMs: 20 
  });

  // 2. Spawn a client attached to Unit ID 1
  const client = transport.create_client({ unitId: 1 });

  try {
    // 3. Read 10 holding registers starting at address 0
    const registers = await client.read_holding_registers(0, 10);
    console.log('Holding registers:', registers); // Uint16Array
  } catch (error) {
    console.error('Failed to read registers:', error);
  }
}
```

### Modbus server demo with modbus-rs-wasm

The `modbus-rs-wasm` package also provides building blocks for server simulation inside the browser. You can instantiate a `WasmTcpServer` or `WasmSerialServer` by providing a configuration and a JavaScript callback to process incoming requests.

Here is a quick example of setting up a simulated server via a WebSocket TCP gateway:

```javascript
import { WasmTcpGatewayConfig, WasmTcpServer } from 'modbus-rs-wasm';

async function simulateServer() {
  // 1. Configure the server (e.g., pointing to a WebSocket gateway)
  const config = new WasmTcpGatewayConfig("ws://localhost:8080");

  // 2. Create the server and define the request handler
  const server = new WasmTcpServer(config, async (request) => {
    console.log("Received Modbus request:", request);
    
    // Simulate processing the request
    // The handler can be fully asynchronous and return a Promise
    return {
      // Return appropriate response fields based on the request
      success: true,
      data: [100, 200, 300]
    };
  });

  // 3. Start the server
  server.start();
  
  // Observe the server status
  console.log("Server running:", server.is_running());
  console.log("Server status:", server.status_snapshot());
}
```

### Serial Server Simulation

You can also create a simulated serial server (RTU or ASCII) and attach a browser `SerialPort` to it using the Web Serial API:

```javascript
import { WasmSerialServerConfig, WasmSerialServer } from 'modbus-rs-wasm';

async function simulateSerialServer() {
  // 1. Request a serial port from the user (requires user gesture)
  const port = await navigator.serial.requestPort();
  
  // 2. Configure the server for RTU or ASCII mode
  const config = WasmSerialServerConfig.rtu(); // or .ascii()

  // 3. Create the server with a request handler callback
  const server = new WasmSerialServer(config, async (request) => {
    console.log("Received Modbus request via Serial:", request);
    return {
      success: true,
      data: [100, 200, 300]
    };
  });

  // 4. Attach the browser serial port
  server.attach_serial_port(port);

  // 5. Start the server
  server.start();
  
  console.log("Serial Server running:", server.is_running());
}
```

## License

GPL-3.0-only — see [LICENSE](./LICENSE). A commercial license is available for proprietary use.
