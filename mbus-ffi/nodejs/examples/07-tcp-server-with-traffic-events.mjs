/**
 * Example 07 — TCP server: traffic events (requires the `nodejs-traffic`
 * Cargo feature when building the native addon)
 *
 * Demonstrates how a server can observe per-request traffic.  The current
 * v0.8 binding routes traffic through the handler callbacks themselves —
 * this example simply logs each handler invocation.  When the
 * `nodejs-traffic` feature is wired up to a dedicated traffic notifier
 * channel, this example will be updated to subscribe to it directly.
 *
 * Build with:  cargo build -p mbus-ffi --features nodejs,nodejs-traffic,full
 * Run with:    node examples/07-tcp-server-with-traffic-events.mjs
 */

import { AsyncTcpModbusServer } from 'modbus-rs';

const counters = { reads: 0, writes: 0 };
const registers = new Array(256).fill(0);

async function main() {
  const server = await AsyncTcpModbusServer.bind(
    { host: '0.0.0.0', port: 5502 },
    {
      onReadHoldingRegisters: (req) => {
        counters.reads++;
        const slice = registers.slice(req.address, req.address + req.quantity);
        return slice;
      },
      onWriteSingleRegister: (req) => {
        counters.writes++;
        registers[req.address] = req.value;
        return true;
      },
      onWriteMultipleRegisters: (req) => {
        counters.writes++;
        for (let i = 0; i < req.values.length; i++) {
          registers[req.address + i] = req.values[i];
        }
        return true;
      },
    },
  );

  console.log('Server listening on :5502 — traffic counters logged every 5 s');

  const ticker = setInterval(() => {
    console.log(`[traffic] reads=${counters.reads} writes=${counters.writes}`);
  }, 5000);

  process.on('SIGINT', async () => {
    clearInterval(ticker);
    await server.shutdown();
    process.exit(0);
  });

  await new Promise(() => {}); // run forever
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
