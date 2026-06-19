# modbus-rs-wasm

Browser-native WebAssembly bindings for [modbus-rs](https://github.com/Raghava-Ch/modbus-rs), enabling Modbus TCP (via WebSockets) and Modbus RTU/ASCII (via Web Serial) directly in the browser.

> **Note:** This package is designed for browser-native environments (using WebSockets and Web Serial). If you are building a Node.js backend application, use the native [`modbus-rs`](https://www.npmjs.com/package/modbus-rs) package instead. Running this package in pure Node.js requires `--experimental-wasm-modules` and is not officially supported.

## Installation

```bash
npm install modbus-rs-wasm
```

## Integration & Frameworks (Vite, Svelte, React, etc.)

No custom resolver aliases are required. Standard package entry points are resolved automatically based on your builder/bundler targets.

### Web (Direct / HTML / Vanilla JS)
When loading the package in browser environments without a bundler (e.g. from CDNs like unpkg or jsDelivr), import the web entry point and await the initialization promise:

```javascript
import init, { WasmTcpTransport } from 'modbus-rs-wasm/web';

await init();
const transport = new WasmTcpTransport('ws://localhost:8502', { 
  responseTimeoutMs: 5000, 
  retryAttempts: 3, 
  tickIntervalMs: 20 
});
const client = transport.create_client({ unitId: 1 });
```

### Bundlers & Modern Frameworks

When using modern bundlers (Vite, Webpack, Next.js, etc.), the root import maps to the bundler target where WebAssembly is loaded automatically:

```javascript
import { WasmTcpTransport } from 'modbus-rs-wasm';
```

Because WASM is an asynchronous module dependency, you must configure your bundler to support WASM loader options:

#### 1. Vite (React, Svelte, Vue, SolidJS, etc. via Vite)
Vite requires helper plugins to support WebAssembly. Install `vite-plugin-wasm` and `vite-plugin-top-level-await`:

```bash
npm install -D vite-plugin-wasm vite-plugin-top-level-await
```

In your `vite.config.ts`, register the plugins and **exclude** `modbus-rs-wasm` from dependency pre-bundling (to prevent Esbuild from trying to optimize the WASM module):

```typescript
import { defineConfig } from 'vite';
import wasm from 'vite-plugin-wasm';
import topLevelAwait from 'vite-plugin-top-level-await';

export default defineConfig({
  plugins: [wasm(), topLevelAwait()],
  optimizeDeps: {
    exclude: ['modbus-rs-wasm']
  }
});
```

#### 2. Webpack 5 (React, Angular, or custom Webpack setups)
Webpack 5 supports WebAssembly natively but it is disabled by default. You need to enable the `asyncWebAssembly` experiment in your `webpack.config.js`:

```javascript
module.exports = {
  // ...
  experiments: {
    asyncWebAssembly: true,
  },
};
```

#### 3. Next.js (Webpack mode)
Configure `next.config.js` to enable WebAssembly support inside Next.js's internal Webpack runner:

```javascript
/** @type {import('next').NextConfig} */
const nextConfig = {
  webpack(config) {
    config.experiments = {
      ...config.experiments,
      asyncWebAssembly: true,
    };
    return config;
  },
};

module.exports = nextConfig;
```

## Quick Start

### Examples
Ready-to-run HTML examples demonstrating both Modbus client and server functionality in the browser:

- **[wasm_client/network_smoke.html](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/wasm/examples/wasm_client/network_smoke.html)**: WebSocket Modbus TCP client example.
- **[wasm_client/serial_smoke.html](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/wasm/examples/wasm_client/serial_smoke.html)**: Web Serial Modbus RTU client example.
- **[wasm_server/network_smoke.html](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/wasm/examples/wasm_server/network_smoke.html)**: WebSocket Modbus TCP server example.
- **[wasm_server/serial_smoke.html](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/wasm/examples/wasm_server/serial_smoke.html)**: Web Serial Modbus RTU server example.
- **[real world example](https://github.com/Raghava-Ch/modbus-lab)**: Comes with demo web client and tauri based desktop client and server simulators with source code.

To run the examples locally:
```bash
# clone the repo
cd modbus-rs/mbus-ffi/wasm
npx serve ./
# Navigate to example folder with chromium based web browser run the examples.
# you can also use the tauri desktop client and server simulator.
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
      retryAttempts: 3
    });

    // 3. Spawn a client for a specific slave (Unit ID)
    const client = transport.createClient({ unitId: 10 });

    // 4. Read coils
    const coils = await client.readCoils({ address: 0, quantity: 8 });
    console.log('Coils:', coils); // boolean[]
    
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
  const transport = await WasmTcpTransport.connect('ws://localhost:8502', { 
    responseTimeoutMs: 5000, 
    retryAttempts: 3
  });

  // 2. Spawn a client attached to Unit ID 1
  const client = transport.createClient({ unitId: 1 });

  try {
    // 3. Read 10 holding registers starting at address 0
    const registers = await client.readHoldingRegisters({ address: 0, quantity: 10 });
    console.log('Holding registers:', registers); // Uint16Array
  } catch (error) {
    console.error('Failed to read registers:', error);
  }
}
```

### Modbus server demo with modbus-rs-wasm

The `modbus-rs-wasm` package also provides building blocks for server simulation inside the browser. You can bind a `WasmTcpServer` or `WasmSerialServer` by providing configurations and an object implementing `ServerHandlers` callback methods.

Here is a quick example of setting up a simulated server via a WebSocket TCP gateway:

```javascript
import { WasmTcpServer } from 'modbus-rs-wasm';

async function simulateServer() {
  // 1. Define the request handlers
  const handlers = {
    onReadHoldingRegisters: async (request) => {
      console.log("Received Modbus request:", request);
      
      // Callbacks can return values synchronously or as Promises
      return [100, 200, 300];
    }
  };

  // 2. Bind the server
  const server = await WasmTcpServer.bind(
    {
      wsUrl: "ws://localhost:8080",
      unitId: 1
    },
    handlers
  );

  // 3. Start the event loop (runs asynchronously in background)
  server.serve().catch(err => {
    console.error("Server crashed:", err);
  });
  
  // To stop the server later:
  // await server.shutdown();
}
```

### Serial Server Simulation

You can also create a simulated serial server (RTU or ASCII) and attach a browser `SerialPort` to it using the Web Serial API:

```javascript
import { WasmSerialServer } from 'modbus-rs-wasm';

async function simulateSerialServer() {
  // 1. Request a serial port from the user (requires user gesture)
  const port = await navigator.serial.requestPort();
  
  // 2. Define the request handlers
  const handlers = {
    onReadHoldingRegisters: (request) => {
      console.log("Received Modbus request via Serial:", request);
      return [100, 200, 300];
    }
  };

  // 3. Bind the server for RTU mode
  const server = await WasmSerialServer.bindRtu(
    {
      serialPort: port,
      unitId: 1,
      baudRate: 19200
    },
    handlers
  );

  // 4. Start the event loop
  server.serve().catch(err => {
    console.error("Serial Server crashed:", err);
  });
}
```

## License

GPL-3.0-only — see [LICENSE](./LICENSE). A commercial license is available for proprietary use.
