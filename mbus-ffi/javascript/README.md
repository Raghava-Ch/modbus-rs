# modbus-rs

High-performance Modbus TCP/RTU/ASCII client, server, and gateway for Node.js and the Browser (via WebAssembly), powered by Rust.

## Features

- **Async/Promise-based API** - All operations return Promises
- **TCP Client** - Full Modbus TCP/IP client implementation (supports communicating with multiple unit IDs behind a single IP address and a serial port)
- **Serial Client** - Modbus RTU and ASCII over serial port
- **TCP & Serial Servers** - Build Modbus TCP servers, or Serial RTU and ASCII servers, using custom JavaScript handlers to respond to incoming requests
- **Modbus Gateway** - Deploy high-performance gateways supporting WebSockets, TCP, and Serial (RTU/ASCII) as upstream channels, and TCP/Serial (RTU/ASCII) as downstream channels, dynamically routing requests based on unit ID mapping tables
- **Automatic WebAssembly Initialization** - Browser WASM transports and servers auto-initialize WebAssembly memory natively on demand
- **Thread Safety & Concurrency** - Rust-backed concurrent architecture ensures safe access across multiple async execution contexts
- **Safety Locks** - Integrated bus locking to prevent command collisions and state corruption
- **Multi-drop Serial Support** - Manage and communicate with multiple device unit IDs on a single physical RTU/ASCII bus
- **High Performance** - Native Rust core with napi-rs bindings for Node.js and wasm-bindgen for the browser
- **Type Safe** - Full TypeScript definitions included
- **Cross Platform** - Pre-built binaries for Linux, macOS, Windows, and WebAssembly. Can also be built locally for platform-specific native Node.js.

## Installation

```bash
npm install modbus-rs
```

## Import Subpaths & Target Selection

| Import Subpath | Target Build | Recommended Use Case |
| :--- | :--- | :--- |
| `import from 'modbus-rs'` | Node.js N-API / Browser Bundler | Auto-selects native Node.js binary or bundler WASM build |
| `import from 'modbus-rs/browser'` | Bundler WASM (`--target bundler`) | Webpack 5, Vite (with `vite-plugin-wasm`), Rollup |
| `import from 'modbus-rs/bundler'` | Bundler WASM (`--target bundler`) | Explicit alias for `--target bundler` |
| `import from 'modbus-rs/web'` | Web WASM (`--target web`) | Raw browser script tags, Vite, or CDN |

---

## Quick Start

### Examples
[https://github.com/Raghava-Ch/modbus-rs/tree/main/mbus-ffi/javascript/examples](https://github.com/Raghava-Ch/modbus-rs/tree/main/mbus-ffi/javascript/examples)

### TCP Client

```javascript
const { AsyncTcpTransport } = require('modbus-rs');

async function main() {
  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: 502,
    requestTimeoutMs: 5000,
  });

  const client = transport.createClient({ unitId: 1 });

  try {
    // Read holding registers (FC03)
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Registers:', registers);

    // Write single register (FC06)
    await client.writeSingleRegister({
      address: 0,
      value: 12345,
    });
  } finally {
    await transport.close();
  }
}

main().catch(console.error);
```

### Serial RTU Client

```javascript
const { AsyncRtuTransport } = require('modbus-rs');

async function main() {
  const transport = await AsyncRtuTransport.open({
    portPath: '/dev/ttyUSB0',
    baudRate: 19200,
    dataBits: 8,
    stopBits: 1,
    parity: 'even',
  });

  const client = transport.createClient({ unitId: 1 });

  try {
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Registers:', registers);
  } finally {
    await transport.close();
  }
}

main().catch(console.error);
```

### TCP Server

```javascript
const { AsyncTcpModbusServer } = require('modbus-rs');

const holdingRegisters = new Uint16Array(1000);

async function main() {
  const server = await AsyncTcpModbusServer.bind(
    { host: '0.0.0.0', port: 502, unitId: 1 },
    {
      onReadHoldingRegisters: (req) => {
        return holdingRegisters.slice(req.address, req.address + req.quantity);
      },
      onWriteSingleRegister: (req) => {
        holdingRegisters[req.address] = req.value;
      },
    }
  );

  console.log('Server listening on port 502');
  
  process.on('SIGINT', async () => {
    await server.shutdown();
    process.exit(0);
  });
}

main().catch(console.error);
```

### TCP Gateway

```javascript
const { AsyncTcpGateway } = require('modbus-rs');

async function main() {
  const gateway = await AsyncTcpGateway.bind(
    { host: '0.0.0.0', port: 502 },
    {
      downstreams: [
        { host: '192.168.1.10', port: 502 },
        { host: '192.168.1.11', port: 502 },
      ],
      routes: [
        { unitId: 1, channel: 0 },
        { unitId: 2, channel: 1 },
      ],
    }
  );

  console.log('Gateway listening on port 502');
  
  process.exit(0);
}

main().catch(console.error);
```

---

## WebAssembly / Browser Usage & Auto-Initialization

### Automatic WebAssembly Initialization (`modbus-rs/web`)

When importing from `'modbus-rs/web'`, **manual `await init()` calls are no longer required**. 

Calling factory entry points (`WasmWsTransport.connect`, `WasmRtuTransport.open`, `WasmWsModbusServer.bind`, `WasmSerialModbusServer.bindRtu`, etc.) automatically triggers internal WebAssembly initialization (`ensureInit`) prior to execution:

```javascript
import { WasmWsTransport } from 'modbus-rs/web';

// No manual await init() needed! WASM auto-initializes on connect():
const transport = await WasmWsTransport.connect({
  wsUrl: 'ws://127.0.0.1:8080/modbus',
  requestTimeoutMs: 3000,
});

const client = transport.createClient({ unitId: 1 });
const registers = await client.readHoldingRegisters({ address: 0, quantity: 10 });
console.log('Registers:', Array.from(registers));

transport.close();
```

### Manual or Custom WASM Binary Path Loading

If you prefer to initialize WebAssembly upfront or provide a custom `.wasm` URL/path, `ensureInit` (or default `init`) remains fully supported:

```javascript
import init, { ensureInit, WasmWsTransport } from 'modbus-rs/web';

// Option A: Explicit initialization upfront
await init();

// Option B: Provide a custom WASM binary URL or path
await ensureInit('/static/custom_modbus_bg.wasm');

const transport = await WasmWsTransport.connect({ wsUrl: 'ws://127.0.0.1:8080' });
```

### Browser / WebAssembly (Web Serial Client)

```javascript
import { requestSerialPort, WasmRtuTransport } from 'modbus-rs/web';

async function connectSerialDevice() {
  // Request Web Serial port handle (must be called from a user gesture)
  const portHandle = await requestSerialPort();

  // Open serial transport for physical RTU serial port (auto-initializes WASM)
  const transport = await WasmRtuTransport.open(portHandle, {
    baudRate: 9600,
    dataBits: 8,
    stopBits: 1,
    parity: 'even',
    requestTimeoutMs: 1000,
  });

  const client = transport.createClient({ unitId: 1 });

  try {
    // Read coils (FC01)
    const coils = await client.readCoils({
      address: 0,
      quantity: 8,
    });
    console.log('Coils:', Array.from(coils));
  } finally {
    transport.close();
  }
}
```

---

## Migration Guide

Detailed step-by-step migration guides are available in the [Migration Guides](https://github.com/Raghava-Ch/modbus-rs/tree/main/documentation/migrations) directory.

---

## Error Handling with Code Constants

Error code constants are exported for both Node.js and Browser targets:

```javascript
import { getModbusErrorCode, ModbusErrorCode } from 'modbus-rs';

try {
  await client.readHoldingRegisters({ address: 0, quantity: 10 });
} catch (err) {
  const code = getModbusErrorCode(err);
  switch (code) {
    case ModbusErrorCode.EXCEPTION:            console.error('Modbus exception'); break;
    case ModbusErrorCode.TIMEOUT:              console.error('Request timed out'); break;
    case ModbusErrorCode.CONNECTION_CLOSED:    console.error('Disconnected'); break;
    default:                                   console.error('Unknown error:', err.message);
  }
}
```

---

## Known Limitations

- **Gateway route limit**: `AsyncTcpGateway` supports a maximum of **64 routing entries**. Attempting to add more will throw at `bind()` time.

> *If any of these limitations are a high priority for your project, please [create a GitHub Issue](https://github.com/Raghava-Ch/modbus-rs/issues).*

---

## API Reference

### AsyncTcpTransport (Node.js)

- `static connect(opts: TcpTransportOptions): Promise<AsyncTcpTransport>` - Connect to a Modbus TCP server
- `close(): Promise<void>` - Close the connection
- `reconnect(): Promise<void>` - Re-establish the connection
- `createClient(opts: CreateClientOptions): AsyncTcpModbusClient` - Create a logical client instance bound to a specific unit ID (required)
- `setRequestTimeout(ms: number): void` - Set a global request timeout (in milliseconds)
- `clearRequestTimeout(): void` - Clear the global request timeout
- `pendingRequests: boolean` - (Getter) Returns whether there are requests currently in flight

### AsyncRtuTransport / AsyncAsciiTransport (Node.js)

- `static open(opts: RtuTransportOptions | AsciiTransportOptions): Promise<AsyncRtuTransport | AsyncAsciiTransport>` - Open the serial port
- `close(): Promise<void>` - Close the connection
- `reconnect(): Promise<void>` - Re-establish the connection
- `createClient(opts: CreateClientOptions): AsyncSerialModbusClient` - Create a logical client instance bound to a specific unit ID (required)
- `setRequestTimeout(ms: number): void` - Set a global request timeout (in milliseconds)
- `clearRequestTimeout(): void` - Clear the global request timeout
- `pendingRequests: boolean` - (Getter) Returns whether there are requests currently in flight

### WasmWsTransport (Browser WebSockets)

- `static connect(opts: WasmWsTransportOptions): Promise<WasmWsTransport>` - Connect to a Modbus WebSocket gateway (auto-initializes WASM)
- `close(): void` - Close the WebSocket connection
- `createClient(opts: CreateClientOptions): WasmWsModbusClient` - Create a logical client instance bound to a specific unit ID (required)

### WasmRtuTransport / WasmAsciiTransport (Browser Web Serial)

- `static open(port: SerialPort, opts: WasmSerialTransportOptions): Promise<WasmRtuTransport | WasmAsciiTransport>` - Open a Web Serial port in RTU or ASCII mode (auto-initializes WASM)
- `close(): void` - Close the serial connection
- `createClient(opts: CreateClientOptions): WasmSerialModbusClient` - Create a logical client instance bound to a specific unit ID (required)

### requestSerialPort (Web Serial Helper)

- `requestSerialPort(): Promise<SerialPort>` - Request Web Serial port handle from user (auto-initializes WASM; must be invoked from a user gesture like a button click)

### WasmWsModbusServer / WasmSerialModbusServer (Browser WebAssembly Servers)

- `static bind(opts, handlers): Promise<WasmWsModbusServer>` - Create and bind a WebSocket Modbus server (auto-initializes WASM)
- `static bindRtu(opts, handlers): Promise<WasmSerialModbusServer>` - Create and bind a Web Serial RTU Modbus server (auto-initializes WASM)
- `static bindAscii(opts, handlers): Promise<WasmSerialModbusServer>` - Create and bind a Web Serial ASCII Modbus server (auto-initializes WASM)
- `serve(): Promise<void>` - Start listening and serving requests
- `shutdown(): void` - Stop the WASM server

### Logical Clients (All Targets)

Logical clients (`AsyncTcpModbusClient`, `AsyncSerialModbusClient`, `WasmWsModbusClient`, `WasmSerialModbusClient`) expose standard Modbus function code methods:

- `readCoils(opts)` - FC01: Read Coils
- `readDiscreteInputs(opts)` - FC02: Read Discrete Inputs
- `readHoldingRegisters(opts)` - FC03: Read Holding Registers
- `readInputRegisters(opts)` - FC04: Read Input Registers
- `writeSingleCoil(opts)` - FC05: Write Single Coil
- `writeSingleRegister(opts)` - FC06: Write Single Register
- `writeMultipleCoils(opts)` - FC15: Write Multiple Coils
- `writeMultipleRegisters(opts)` - FC16: Write Multiple Registers
- `readWriteMultipleRegisters(opts)` - FC23: Read/Write Multiple Registers
- `readFileRecord(opts)` - FC20: Read File Record
- `writeFileRecord(opts)` - FC21: Write File Record
- `readFifoQueue(opts)` - FC24: Read FIFO Queue
- `readExceptionStatus()` - FC07: Read Exception Status
- `diagnostics(opts)` - FC08: Diagnostics
- `readDeviceIdentification(opts)` - FC43/14: Read Device Identification

### AsyncTcpModbusServer (Node.js)

- `static bind(opts, handlers): Promise<AsyncTcpModbusServer>` - Create and start a TCP server
- `shutdown(): Promise<void>` - Stop the server

### AsyncSerialModbusServer (Node.js)

- `static bindRtu(opts, handlers): Promise<AsyncSerialModbusServer>` - Create and start a Serial RTU server
- `static bindAscii(opts, handlers): Promise<AsyncSerialModbusServer>` - Create and start a Serial ASCII server
- `shutdown(): Promise<void>` - Stop the server

### AsyncTcpGateway (Node.js)

- `static bind(opts, config): Promise<AsyncTcpGateway>` - Create and start a gateway
- `shutdown(): Promise<void>` - Stop the gateway

---

## Supported Platforms

Pre-built binaries are published for:

- Linux x64 (glibc), Linux arm64 (glibc)
- macOS x64, macOS arm64
- Windows x64 (MSVC)
- WebAssembly / Browser

Other targets can be built locally via `cargo build -p mbus-ffi --features nodejs,full`
followed by `npm run build`.

---

## License

GPL-3.0-only — see [LICENSE](./LICENSE).
A commercial license is available for proprietary use; contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com).
