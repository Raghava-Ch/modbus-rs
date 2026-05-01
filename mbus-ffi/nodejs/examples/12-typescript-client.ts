/**
 * Example 12 — TypeScript usage
 *
 * Demonstrates the strongly-typed API surface.  Every option object,
 * return value, and class is fully typed via the bundled `index.d.ts`.
 *
 * Build & run:
 *   npm install --no-save tsx
 *   npx tsx examples/12-typescript-client.ts
 *
 * Or compile first with `npx tsc --project tsconfig.json` and run the
 * emitted JavaScript with `node`.
 */

import {
  AsyncTcpModbusClient,
  type TcpClientOptions,
  type ReadRegistersOptions,
} from 'modbus-rs';

const opts: TcpClientOptions = {
  host: process.env.MODBUS_HOST ?? '127.0.0.1',
  port: Number(process.env.MODBUS_PORT ?? 5502),
  unitId: 1,
  timeoutMs: 2000,
};

async function main(): Promise<void> {
  const client: AsyncTcpModbusClient = await AsyncTcpModbusClient.connect(opts);

  try {
    const readReq: ReadRegistersOptions = { address: 0, quantity: 4 };
    const regs: number[] = await client.readHoldingRegisters(readReq);
    console.log('regs:', regs);

    await client.writeMultipleRegisters({
      address: 0,
      values: regs.map((v) => (v + 1) & 0xffff),
    });
    console.log('Incremented and wrote back.');
  } finally {
    await client.close();
  }
}

main().catch((err: Error) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
