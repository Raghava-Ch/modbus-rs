# WebAssembly (WASM) Bindings

The `mbus-ffi` crate provides native WebAssembly bindings, allowing the powerful `modbus-rs` core logic to run directly inside web browsers and modern JS bundlers. These bindings are published to npm as `modbus-rs-wasm`.

## Architecture & Transport Multiplexing

In a browser environment, raw TCP and Serial sockets are not natively available due to security sandboxes. Instead, the WASM bindings seamlessly map Modbus protocols over browser-native APIs:
- **WebSocket Transport (`WasmTcpTransport`)**: Maps Modbus TCP payloads over WebSockets. This is designed to connect to the [modbus-gateway](https://github.com/Raghava-Ch/modbus-gateway) application running as an upstream proxy.
- **Web Serial Transport (`WasmSerialTransport`)**: Maps Modbus RTU and ASCII over the browser's [Web Serial API](https://developer.mozilla.org/en-US/docs/Web/API/Web_Serial_API).

### Multi-Drop / Multiplexing Design

The architecture explicitly separates **Transports** from **Clients/Servers**. 

A single physical connection (e.g., one selected Serial Port or one WebSocket connection) is owned by a Transport. You can spawn multiple lightweight Modbus clients from a single transport, each bound to a different `Unit ID`. 

The transport manages the underlying request/response queue, the internal polling tick loops, and safely dispatches responses back to the correct client based on the Modbus Transaction ID (for TCP) or sequential queuing (for RTU/ASCII).

## Cargo Features

The WASM bindings are strictly feature-gated. If you are building from source, configure Cargo with the following features:
- `wasm-client`: Enables WASM bindings for Modbus clients (TCP and Serial).
- `wasm-server`: Enables WASM bindings for Modbus servers.
- `wasm-full`: Convenience alias enabling all WASM features, alongside all Modbus function codes (coils, registers, diagnostics).

## Building from Source

To compile the `mbus-ffi` crate to WASM, use the standard [`wasm-pack`](https://rustwasm.github.io/wasm-pack/) toolchain:

```bash
# Build for web (Direct import without a bundler)
wasm-pack build mbus-ffi/ --target web --out-name modbus-rs --out-dir wasm/dist/web --features wasm-full

# Build for modern bundlers (Vite, Next.js, SvelteKit, Webpack)
wasm-pack build mbus-ffi/ --target bundler --out-name modbus-rs --out-dir wasm/dist/bundler --features wasm-full
```

## Running Tests & Troubleshooting ChromeDriver

End-to-End WASM tests execute in a headless Google Chrome environment via the `wasm-bindgen-test-runner` to accurately simulate browser APIs.

**Run Browser E2E Tests:**
```bash
wasm-pack test --chrome --headless mbus-ffi/ --features wasm-full --test wasm_e2e
```

**Run Server/Node Tests:**
```bash
wasm-pack test --node mbus-ffi/ --features wasm-full --test wasm_server_bindings
```

> [!WARNING]
> ### MacOS ChromeDriver Mismatch Issue
> When running `wasm-pack test --chrome --headless` locally on macOS, you may encounter a crash error stating `driver status: signal: 9 (SIGKILL)` and `Error: http status: 404`. 
> 
> This is caused by a known bug in `wasm-pack` where it downloads an outdated, broken version of `chromedriver` (e.g., v143) and attempts to run it against modern versions of Google Chrome (v149+).
> 
> **The Fix:** Bypass the `wasm-pack` auto-downloader by installing `chromedriver` globally via npm, and forcing `wasm-pack` to use it via the `CHROMEDRIVER` environment variable:
> ```bash
> # 1. Install correct driver version globally
> npm install -g chromedriver
> 
> # 2. Run the tests utilizing the newly installed driver path
> CHROMEDRIVER=$(which chromedriver) wasm-pack test --chrome --headless mbus-ffi/ --features wasm-full --test wasm_e2e
> ```

## Quick Start Examples

### Modbus TCP Client (WebSocket)

```javascript
import { WasmTcpTransport } from 'modbus-rs-wasm';

// 1. Create a WebSocket transport (ws_url, options)
const transport = new WasmTcpTransport('ws://localhost:8502', { 
  responseTimeoutMs: 5000, 
  retryAttempts: 3, 
  tickIntervalMs: 20 
});

// 2. Spawn a lightweight client attached to Unit ID 1
const client1 = transport.create_client({ unitId: 1 });

// 3. Spawn a second client attached to Unit ID 2 on the SAME transport
const client2 = transport.create_client({ unitId: 2 });

// 4. Execute requests concurrently! The transport handles the multiplexing.
const [regs1, regs2] = await Promise.all([
    client1.read_holding_registers(0, 10),
    client2.read_holding_registers(10, 10)
]);

console.log('Unit 1 Registers:', regs1);
console.log('Unit 2 Registers:', regs2);
```

### Modbus RTU Client (Web Serial)

*Note: Accessing the Web Serial API must be triggered by an explicit user gesture (e.g., a button click).*

```javascript
import { request_serial_port, WasmSerialTransport } from 'modbus-rs-wasm';

document.getElementById('connect-btn').addEventListener('click', async () => {
  try {
    // 1. Prompt the browser to show the port selection dialog
    const portHandle = await request_serial_port();
    
    // 2. Create the RTU Transport and claim the port
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

    // 3. Spawn a client for a specific slave
    const client = transport.create_client({ unitId: 10 });
    
    // 4. Communicate
    const coils = await client.read_coils(0, 8);
    console.log('Coils:', coils); // Uint8Array
    
  } catch (err) {
    console.error('Serial connection error:', err);
  }
});
```
