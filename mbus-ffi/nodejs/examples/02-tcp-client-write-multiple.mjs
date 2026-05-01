/**
 * Example 02 — TCP client: write multiple registers (FC16)
 *
 * Demonstrates writing a block of holding registers, then reading them
 * back to verify.
 *
 * Run:    node examples/02-tcp-client-write-multiple.mjs
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
    const values = [10, 20, 30, 40, 50, 60, 70, 80];
    await client.writeMultipleRegisters({ address: 100, values });
    console.log(`Wrote ${values.length} registers @ 100:`, values);

    const readBack = await client.readHoldingRegisters({
      address: 100,
      quantity: values.length,
    });
    console.log('Read back:', readBack);

    const ok = readBack.every((v, i) => v === values[i]);
    console.log(ok ? '✓ Round trip OK' : '✗ Mismatch!');
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
