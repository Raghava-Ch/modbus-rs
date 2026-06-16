/**
 * Example 13 — TypeScript Server: basic in-memory store
 *
 * Spawns an in-process Modbus TCP server on port 5502 backed by a
 * simple typed store, demonstrating type-safe server handlers.
 *
 * Build & run:
 *   npm install --no-save tsx
 *   npx tsx examples/13-typescript-server.ts
 *
 * Or compile first with `npx tsc --project tsconfig.json` and run the
 * emitted JavaScript with `node`.
 */

import {
  AsyncTcpModbusServer,
  ServerHandlers,
  ReadHoldingRegistersRequest,
  WriteMultipleRegistersRequest,
  ModbusException
} from 'modbus-rs';

const PORT = Number(process.env.MODBUS_PORT ?? 5502);

// Simulated registers
const holdingRegisters = new Uint16Array(1000);

async function main(): Promise<void> {
  const handlers: ServerHandlers = {
    // Read Holding Registers (FC03)
    onReadHoldingRegisters: (req: ReadHoldingRegistersRequest): number[] | ModbusException => {
      console.log(`Read registers: unit=${req.unitId} addr=${req.address} qty=${req.quantity}`);
      if (req.address + req.quantity > holdingRegisters.length) {
        // Return Modbus Exception 2 (Illegal Data Address)
        return { exception: 2 };
      }
      return Array.from(holdingRegisters.slice(req.address, req.address + req.quantity));
    },

    // Write Multiple Registers (FC16)
    onWriteMultipleRegisters: (req: WriteMultipleRegistersRequest): void | ModbusException => {
      console.log(`Write registers: unit=${req.unitId} addr=${req.address} count=${req.values.length}`);
      if (req.address + req.values.length > holdingRegisters.length) {
        return { exception: 2 };
      }
      for (let i = 0; i < req.values.length; i++) {
        holdingRegisters[req.address + i] = req.values[i];
      }
    }
  };

  console.log('Binding server on port', PORT);
  const server = await AsyncTcpModbusServer.bind(
    {
      host: '0.0.0.0',
      port: PORT,
      unitId: 1
    },
    handlers
  );

  console.log('TypeScript TCP server listening on port', PORT);
  console.log('Press Ctrl+C to stop');

  process.on('SIGINT', async () => {
    console.log('\nShutting down server...');
    await server.shutdown();
    process.exit(0);
  });

  // Keep alive
  await new Promise<void>(() => {});
}

main().catch((err: Error) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
