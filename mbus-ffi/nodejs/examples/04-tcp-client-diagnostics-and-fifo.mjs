/**
 * Example 04 — TCP client: diagnostics (FC08) and FIFO queue (FC18)
 *
 * Demonstrates the less-common function codes used for device introspection.
 * Note: not all servers/devices implement these; expect ILLEGAL_FUNCTION
 * exceptions if they don't.
 *
 * Run:    node examples/04-tcp-client-diagnostics-and-fifo.mjs
 */

import { AsyncTcpModbusClient } from 'modbus-rs';

const HOST = process.env.MODBUS_HOST ?? '127.0.0.1';
const PORT = Number(process.env.MODBUS_PORT ?? 5502);

async function main() {
  const client = await AsyncTcpModbusClient.connect({
    host: HOST,
    port: PORT,
    unitId: 1,
    timeoutMs: 2000,
  });

  try {
    // FC08 — Diagnostics, sub-function 0x0000 = Return Query Data (loopback)
    try {
      const diag = await client.diagnostics({ subFunction: 0x0000, data: 0xCAFE });
      console.log(`FC08 loopback echo: sub=${diag.subFunction.toString(16)} data=0x${diag.data.toString(16)}`);
    } catch (err) {
      console.log('FC08 not supported by this server:', err.message);
    }

    // FC07 — Read Exception Status (1-byte coil-bitmap)
    try {
      const status = await client.readExceptionStatus();
      console.log(`FC07 exception status: 0b${status.toString(2).padStart(8, '0')}`);
    } catch (err) {
      console.log('FC07 not supported by this server:', err.message);
    }

    // FC18 — Read FIFO Queue
    try {
      const fifo = await client.readFifoQueue({ address: 0 });
      console.log(`FC18 FIFO @ 0 (count=${fifo.count}):`, fifo.values);
    } catch (err) {
      console.log('FC18 not supported by this server:', err.message);
    }
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
