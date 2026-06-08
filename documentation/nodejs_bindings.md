# Node.js Bindings

The Node.js bindings expose `modbus-rs` through a native addon built with
[napi-rs](https://napi.rs/) and published to npm as
[`modbus-rs`](https://www.npmjs.com/package/modbus-rs).

They follow the same native-binding architecture as the .NET and Go
bindings — a thin layer over the existing `mbus-client-async`,
`mbus-server-async`, and `mbus-gateway` crates with a single shared
Tokio runtime and opaque handles. The JavaScript surface is idiomatic
JS: classes with async methods, plain options objects, and JS errors.

No Modbus protocol logic is reimplemented in JavaScript. Requests flow
through:

```text
JS public API → #[napi] wrapper → async Rust crates → wire
```

## Status

Implemented:

| Class | Purpose | Notes |
|---|---|---|
| `AsyncTcpTransport` | Manage physical TCP connections | Factory for client handles |
| `AsyncRtuTransport` | Manage physical RTU serial port connections | Factory for client handles |
| `AsyncAsciiTransport` | Manage physical ASCII serial port connections | Factory for client handles |
| `AsyncTcpModbusClient` | Lightweight logical TCP client handle | Extracted from transport via `createClient()` |
| `AsyncSerialModbusClient` | Lightweight logical Serial RTU/ASCII client handle | Extracted from transport via `createClient()` |
| `AsyncTcpModbusServer` | Async Modbus TCP server | Drives request handling via JS handler callback dispatch |
| `AsyncTcpGateway` | Async Modbus TCP gateway with unit-ID routing | Routing table implemented |

## Building

You need Node.js ≥ 24.6 LTS and a working Rust toolchain. On Linux you also
need `libudev-dev` (Debian/Ubuntu) or `libudev-devel` (Fedora/RHEL) for
the serialport dependency.

```bash
# 1) Build the native addon
cd mbus-ffi/nodejs
npm install
npm run build
```

Tests use Node's built-in `node:test` runner so no extra test framework
is required:

```bash
npm test
```

## Quick start

```js
import { AsyncTcpTransport, AsyncTcpModbusServer } from 'modbus-rs';

// Server (bind is synchronous, requires unitId)
const server = AsyncTcpModbusServer.bind(
  { host: '0.0.0.0', port: 5502, unitId: 1 },
  {
    onReadHoldingRegisters: ({ address, quantity }) =>
      Array.from({ length: quantity }, (_, i) => address + i),
  },
);

// Client transport connection
const transport = await AsyncTcpTransport.connect({
  host: '127.0.0.1',
  port: 5502,
  timeoutMs: 2000,
});

// Create logical client from transport
const client = transport.createClient({ unitId: 1 });

const regs = await client.readHoldingRegisters({ address: 0, quantity: 4 });
console.log(regs); // [0, 1, 2, 3]

await transport.close();
await server.shutdown();
```

## Examples

A self-contained tour of every API lives in
[`mbus-ffi/nodejs/examples/`](../mbus-ffi/nodejs/examples/) — twelve
examples covering TCP client, TCP server, gateway, both serial modes,
and a TypeScript example. See
[the examples README](../mbus-ffi/nodejs/examples/README.md) for the
full index and instructions for running the serial examples (which need
either a real serial device or a virtual port + simulator like
`socat` + `diagslave` on Linux/macOS or `com0com` + `Modbus Slave` on
Windows).

## TypeScript

`index.d.ts` is committed to the repository and shipped in the npm
package, so consumers get type-checking out of the box without any extra
configuration.

## Cargo features

| Feature | What it pulls in |
|---|---|
| `nodejs` | The napi-rs binding code (depends on `tokio`, all Modbus data features, and the async client/server/gateway crates). |
| `nodejs-traffic` | Adds traffic notifier support (`mbus-server-async/traffic` + `mbus-client-async/traffic`). |

The `nodejs` feature is **not** in `default`; the addon is built with
`cargo build -p mbus-ffi --features nodejs,full` (driven automatically
by `npm run build`).
