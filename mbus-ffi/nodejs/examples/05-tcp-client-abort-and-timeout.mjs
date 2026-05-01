/**
 * Example 05 — TCP client: per-request timeout
 *
 * Demonstrates `timeoutMs` at connect time.  If the server doesn't reply
 * within the timeout, the request rejects with a timeout error.
 *
 * Note: AbortSignal-based per-call cancellation is on the roadmap but not
 * yet implemented in the v0.8 Node.js bindings.  Use `timeoutMs` in the
 * constructor for now.
 *
 * Run:    node examples/05-tcp-client-abort-and-timeout.mjs
 */

import { AsyncTcpModbusClient } from 'modbus-rs';

const HOST = process.env.MODBUS_HOST ?? '127.0.0.1';
const PORT = Number(process.env.MODBUS_PORT ?? 5502);

async function main() {
  // Aggressive 50 ms timeout: works against a healthy local server, will
  // reject quickly against a slow / non-responsive one.
  const client = await AsyncTcpModbusClient.connect({
    host: HOST,
    port: PORT,
    unitId: 1,
    timeoutMs: 50,
  });

  try {
    const t0 = Date.now();
    const regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
    console.log(`Read ${regs.length} register(s) in ${Date.now() - t0} ms:`, regs);
  } catch (err) {
    console.error(`Request failed (expected if server is slow): ${err.message}`);
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
