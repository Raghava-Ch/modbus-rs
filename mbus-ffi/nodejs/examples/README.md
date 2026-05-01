# `modbus-rs` Node.js examples

A self-contained tour of the `modbus-rs` Node.js bindings.  Every example is
runnable on its own with `node examples/<file>.mjs` after the package has
been installed (`npm install`) and built (`npm run build`).

## Index

| #  | File | Topic |
|----|------|-------|
| 01 | [`01-tcp-client-read-holding.mjs`](./01-tcp-client-read-holding.mjs) | TCP client — read holding registers (FC03) and other reads |
| 02 | [`02-tcp-client-write-multiple.mjs`](./02-tcp-client-write-multiple.mjs) | TCP client — write multiple registers (FC16) and read back |
| 03 | [`03-tcp-client-coils-and-discrete-inputs.mjs`](./03-tcp-client-coils-and-discrete-inputs.mjs) | TCP client — coils (FC01/05/15) and discrete inputs (FC02) |
| 04 | [`04-tcp-client-diagnostics-and-fifo.mjs`](./04-tcp-client-diagnostics-and-fifo.mjs) | TCP client — diagnostics (FC08), exception status (FC07), FIFO (FC18) |
| 05 | [`05-tcp-client-abort-and-timeout.mjs`](./05-tcp-client-abort-and-timeout.mjs) | TCP client — per-request timeout (AbortSignal coming later) |
| 06 | [`06-tcp-server-basic.mjs`](./06-tcp-server-basic.mjs) | TCP server — in-memory store with handlers for every FC |
| 07 | [`07-tcp-server-with-traffic-events.mjs`](./07-tcp-server-with-traffic-events.mjs) | TCP server — traffic counters (build with `nodejs-traffic` for full event stream) |
| 08 | [`08-tcp-gateway.mjs`](./08-tcp-gateway.mjs) | TCP gateway — route by unit ID across multiple downstream servers |
| 09 | [`09-serial-rtu-client.mjs`](./09-serial-rtu-client.mjs) | Serial client — RTU mode (requires a serial port — see header) |
| 10 | [`10-serial-rtu-server.mjs`](./10-serial-rtu-server.mjs) | Serial server — placeholder; not yet exposed in Node bindings (v0.8) |
| 11 | [`11-serial-ascii-client.mjs`](./11-serial-ascii-client.mjs) | Serial client — ASCII mode |
| 12 | [`12-typescript-client.ts`](./12-typescript-client.ts) | TypeScript example — strongly-typed API |

## Running examples 01-08 (TCP)

The TCP client examples (01-05) need a Modbus TCP server.  The simplest
setup is to run the in-process server example in one terminal:

```bash
node examples/06-tcp-server-basic.mjs
```

…and in another terminal, run any of the client examples:

```bash
node examples/01-tcp-client-read-holding.mjs
```

You can override the target with environment variables:

```bash
MODBUS_HOST=192.168.1.50 MODBUS_PORT=502 node examples/01-tcp-client-read-holding.mjs
```

## Running serial examples (09-11)

Serial examples need a real or virtual serial port plus a Modbus
device or simulator on the other end.  See the header comment of
[`09-serial-rtu-client.mjs`](./09-serial-rtu-client.mjs) for full setup
instructions covering:

* USB-to-RS485 dongles connected to a real PLC / device.
* Linux/macOS virtual ports via `socat` paired with `diagslave`,
  `pyModSlave`, or [`modbus-lab`](https://github.com/modbus-lab).
* Windows virtual ports via [com0com](https://com0com.sourceforge.net/) +
  `Modbus Slave` (modbustools.com).

Set the `PORT` env-var to your port path:

```bash
PORT=/dev/ttyUSB0    node examples/09-serial-rtu-client.mjs   # Linux
PORT=/dev/cu.usbserial-XXXX node examples/09-serial-rtu-client.mjs # macOS
PORT=COM3            node examples/09-serial-rtu-client.mjs   # Windows
```

## TypeScript example (12)

```bash
# One-off run with tsx (no compile step):
npx tsx examples/12-typescript-client.ts

# Or compile first:
npx tsc --project tsconfig.json
node dist/examples/12-typescript-client.js
```

## Troubleshooting

* **`Error: Cannot find module 'modbus-rs'`** — run `npm install` then
  `npm run build` to compile the native addon for your platform.  See the
  package README for prerequisites (Rust toolchain, `libudev-dev` on Linux).
* **Connection refused** — make sure a server is listening on the host/port.
* **Serial port not found** — check the path with `ls /dev/tty*` (Linux/macOS)
  or `mode` (Windows), and confirm the user has access (Linux: add user to
  `dialout` group).
