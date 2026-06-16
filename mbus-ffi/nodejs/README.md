# modbus-rs

High-performance Modbus TCP/RTU/ASCII client, server, and gateway for Node.js, powered by Rust.

## Features

- **Async/Promise-based API** - All operations return Promises
- **TCP Client** - Full Modbus TCP/IP client implementation (supports communicating with multiple unit IDs behind a single IP address and port)
- **Serial Client** - Modbus RTU and ASCII over serial port
- **TCP & Serial Servers** - Build Modbus TCP servers, or Serial RTU and ASCII servers, using custom JavaScript handlers to respond to incoming requests
- **Modbus Gateway** - Deploy high-performance gateways supporting WebSockets, TCP, and Serial (RTU/ASCII) as both upstream and downstream channels, dynamically routing requests based on unit ID mapping tables
- **Thread Safety & Concurrency** - Rust-backed concurrent architecture ensures safe access across multiple async execution contexts
- **Safety Locks** - Integrated bus locking to prevent command collisions and state corruption
- **Multi-drop Serial Support** - Manage and communicate with multiple device unit IDs on a single physical RTU/ASCII bus
- **High Performance** - Native Rust core with napi-rs bindings
- **Type Safe** - Full TypeScript definitions included
- **Cross Platform** - Pre-built binaries for Linux, macOS, and Windows

## Installation

```bash
npm install modbus-rs
```

## Quick Start

### Examples
[https://github.com/Raghava-Ch/modbus-rs/tree/main/mbus-ffi/nodejs/examples](https://github.com/Raghava-Ch/modbus-rs/tree/main/mbus-ffi/nodejs/examples)

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

const holdingRegisters = new Array(1000).fill(0);

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
  
  process.on('SIGINT', async () => {
    await gateway.shutdown();
    process.exit(0);
  });
}

main().catch(console.error);
```

## Migration Guide

### v0.15 Breaking Changes

#### `TcpTransportOptions.timeoutMs` renamed to `requestTimeoutMs`

```js
// v0.14 (old)
const transport = await AsyncTcpTransport.connect({
  host: '127.0.0.1',
  port: 502,
  timeoutMs: 5000,   // ← old name
});

// v0.15+ (new)
const transport = await AsyncTcpTransport.connect({
  host: '127.0.0.1',
  port: 502,
  requestTimeoutMs: 5000,   // ← consistent with all other transports
});
```

#### Server `bind()` / `bindRtu()` / `bindAscii()` are now async

```js
// v0.14 (old)
const server = AsyncTcpModbusServer.bind(opts, handlers);  // sync

// v0.15+ (new)
const server = await AsyncTcpModbusServer.bind(opts, handlers);  // async, throws on bind failure
```

This change allows bind errors (port already in use, invalid address, etc.) to propagate to your code instead of being silently swallowed.

## Error Handling with Code Constants

Error code constants are now exported:

```js
const { getModbusErrorCode, ModbusErrorCode } = require('modbus-rs');

try {
  await client.readHoldingRegisters({ address: 0, quantity: 10 });
} catch (err) {
  const code = getModbusErrorCode(err);
  switch (code) {
    case ModbusErrorCode.EXCEPTION:      console.error('Modbus exception'); break;
    case ModbusErrorCode.TIMEOUT:        console.error('Request timed out'); break;
    case ModbusErrorCode.CONNECTION_CLOSED: console.error('Disconnected'); break;
    default:                             console.error('Unknown error:', err.message);
  }
}
```

## Known Limitations

- **AbortSignal**: Uses `signal.onabort` instead of `signal.addEventListener`. Only one abort handler per signal object is supported.
- **Gateway route limit**: `AsyncTcpGateway` supports a maximum of **16 routing entries**. Attempting to add more will throw at `bind()` time.
- **Server file record handlers (FC20/FC21)**: The server-side handler receives a simplified request structure. Complex multi-sub-request scenarios are supported.

## API Reference

### AsyncTcpTransport

- `static connect(opts: TcpTransportOptions): Promise<AsyncTcpTransport>` - Connect to a Modbus TCP server
- `close(): Promise<void>` - Close the connection
- `reconnect(): Promise<void>` - Re-establish the connection
- `createClient(opts: CreateClientOptions): AsyncTcpModbusClient` - Create a logical client instance bound to a specific unit ID (required)
- `setRequestTimeout(ms: number): void` - Set a global request timeout (in milliseconds)
- `clearRequestTimeout(): void` - Clear the global request timeout
- `pendingRequests: boolean` - (Getter) Returns whether there are requests currently in flight

### AsyncRtuTransport / AsyncAsciiTransport

- `static open(opts: RtuTransportOptions | AsciiTransportOptions): Promise<AsyncRtuTransport | AsyncAsciiTransport>` - Open the serial port
- `close(): Promise<void>` - Close the connection
- `reconnect(): Promise<void>` - Re-establish the connection
- `createClient(opts: CreateClientOptions): AsyncSerialModbusClient` - Create a logical client instance bound to a specific unit ID (required)
- `setRequestTimeout(ms: number): void` - Set a global request timeout (in milliseconds)
- `clearRequestTimeout(): void` - Clear the global request timeout
- `pendingRequests: boolean` - (Getter) Returns whether there are requests currently in flight

### AsyncTcpModbusClient / AsyncSerialModbusClient

These logical clients contain all the Modbus function code methods:

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

### AsyncTcpModbusServer

- `static bind(opts, handlers): Promise<AsyncTcpModbusServer>` - Create and start a TCP server
- `shutdown(): Promise<void>` - Stop the server

### AsyncSerialModbusServer

- `static bindRtu(opts, handlers): Promise<AsyncSerialModbusServer>` - Create and start a Serial RTU server
- `static bindAscii(opts, handlers): Promise<AsyncSerialModbusServer>` - Create and start a Serial ASCII server
- `shutdown(): Promise<void>` - Stop the server

### AsyncTcpGateway

- `static bind(opts, config): Promise<AsyncTcpGateway>` - Create and start a gateway
- `shutdown(): Promise<void>` - Stop the gateway

## Supported platforms

Pre-built binaries are published for:

- Linux x64 (glibc), Linux arm64 (glibc)
- macOS x64, macOS arm64
- Windows x64 (MSVC)
- WebAssembly [modbus-rs-wasm](https://www.npmjs.com/package/modbus-rs-wasm)

Other targets can be built locally via `cargo build -p mbus-ffi --features nodejs,full`
followed by `npm run build`.

## License

GPL-3.0-only — see [LICENSE](./LICENSE).
A commercial license is available for proprietary use; contact ch.raghava44@gmail.com.
