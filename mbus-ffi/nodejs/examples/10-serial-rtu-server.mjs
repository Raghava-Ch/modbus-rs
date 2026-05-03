/**
 * Example 10 — Serial RTU server
 *
 * ───────────────────────────────────────────────────────────────────────────
 * REQUIREMENTS — same as Example 09 (see its header for full setup notes).
 * ───────────────────────────────────────────────────────────────────────────
 *
 * NOTE — As of v0.8 the Node.js bindings expose `AsyncTcpModbusServer` but
 * not yet `AsyncSerialModbusServer`.  Serial server support is planned for
 * v0.9 and will look like:
 *
 *     import { AsyncSerialModbusServer } from 'modbus-rs';
 *     const server = await AsyncSerialModbusServer.bindRtu(
 *       { portPath: '/dev/ttyUSB0', unitId: 1, baudRate: 19200, parity: 'even' },
 *       { onReadHoldingRegisters: (req) => [...] },
 *     );
 *
 * In the meantime, you can run a serial server using the underlying Rust
 * crate directly — see `mbus-async/examples/serial_server.rs` — or use the
 * TCP server (Example 06) with a TCP-to-serial gateway.
 *
 * Run:    node examples/10-serial-rtu-server.mjs   (prints this notice)
 */

console.log('Serial server is not yet available in the Node.js bindings (v0.8).');
console.log('See the header comment in this file for a workaround.');
process.exit(0);
