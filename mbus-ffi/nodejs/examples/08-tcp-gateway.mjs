/**
 * Example 08 — TCP gateway: route by unit ID
 *
 * Listens on :5502 and forwards Modbus requests to one of several
 * downstream Modbus TCP servers based on the request's unit ID.
 *
 * Run:    node examples/08-tcp-gateway.mjs
 */

import { AsyncTcpGateway } from 'modbus-rs';

async function main() {
  // Create and bind the gateway
  const gateway = await AsyncTcpGateway.bind(
    {
      host: '0.0.0.0',
      port: 5502,
    },
    {
      // List of downstream Modbus servers
      downstreams: [
        { host: '192.168.1.10', port: 502 },  // Channel 0
        { host: '192.168.1.11', port: 502 },  // Channel 1
        { host: '192.168.1.12', port: 502 },  // Channel 2
      ],
      // Route unit IDs to downstream channels
      routes: [
        { unitId: 1, channel: 0 },   // Unit 1 → Channel 0
        { unitId: 2, channel: 0 },   // Unit 2 → Channel 0
        { unitId: 10, channel: 1 },  // Unit 10 → Channel 1
        { unitId: 11, channel: 1 },  // Unit 11 → Channel 1
        { unitId: 20, channel: 2 },  // Unit 20 → Channel 2
      ],
    }
  );

  console.log('Modbus TCP gateway listening on port 5502');
  console.log('Routing:');
  console.log('  Units 1-2  → 192.168.1.10:502');
  console.log('  Units 10-11 → 192.168.1.11:502');
  console.log('  Unit 20    → 192.168.1.12:502');
  console.log('Press Ctrl+C to stop');

  // Handle shutdown
  process.on('SIGINT', async () => {
    console.log('\nShutting down gateway...');
    await gateway.shutdown();
    process.exit(0);
  });

  // Keep the process alive
  await new Promise(() => {});
}

main().catch(console.error);
