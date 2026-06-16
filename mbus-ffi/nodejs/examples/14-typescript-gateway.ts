/**
 * Example 14 — TypeScript Gateway: route by unit ID
 *
 * Listens on :5502 and forwards Modbus requests to one of several
 * downstream Modbus TCP servers based on the request's unit ID,
 * demonstrating typed configurations.
 * Build & run:
 *   npm install --no-save tsx
 *   npx tsx examples/14-typescript-gateway.ts
 *
 * Or compile first with `npx tsc --project tsconfig.json` and run the
 * emitted JavaScript with `node`.
 */

import {
  AsyncTcpGateway,
  GatewayBindOptions,
  GatewayConfig
} from 'modbus-rs';

const PORT = Number(process.env.MODBUS_PORT ?? 5502);

async function main(): Promise<void> {
  const bindOpts: GatewayBindOptions = {
    host: '0.0.0.0',
    port: PORT
  };

  const gatewayConfig: GatewayConfig = {
    // List of downstream Modbus servers
    downstreams: [
      { host: '192.168.1.10', port: 502 },  // Channel 0
      { host: '192.168.1.11', port: 502 },  // Channel 1
      { host: '192.168.1.12', port: 502 }   // Channel 2
    ],
    // Route unit IDs to downstream channels
    routes: [
      { unitId: 1, channel: 0 },   // Unit 1 → Channel 0
      { unitId: 2, channel: 0 },   // Unit 2 → Channel 0
      { unitId: 10, channel: 1 },  // Unit 10 → Channel 1
      { unitId: 11, channel: 1 },  // Unit 11 → Channel 1
      { unitId: 20, channel: 2 }   // Unit 20 → Channel 2
    ]
  };

  console.log('Binding gateway on port', PORT);
  const gateway = await AsyncTcpGateway.bind(bindOpts, gatewayConfig);

  console.log('TypeScript Modbus TCP gateway listening on port', PORT);
  console.log('Routing:');
  console.log('  Units 1-2  → 192.168.1.10:502');
  console.log('  Units 10-11 → 192.168.1.11:502');
  console.log('  Unit 20    → 192.168.1.12:502');
  console.log('Press Ctrl+C to stop');

  process.on('SIGINT', async () => {
    console.log('\nShutting down gateway...');
    await gateway.shutdown();
    process.exit(0);
  });

  // Keep alive
  await new Promise<void>(() => {});
}

main().catch((err: Error) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
