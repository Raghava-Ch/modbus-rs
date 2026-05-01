/**
 * Example 06 — TCP server: basic in-memory store
 *
 * Spawns an in-process Modbus TCP server on port 5502 backed by a
 * simple JavaScript object that stores coils and registers in memory.
 * Run any of the client examples (01-05) against it.
 *
 * Run:    node examples/06-tcp-server-basic.mjs
 *         (then in another terminal:)
 *         node examples/01-tcp-client-read-holding.mjs
 */

import { AsyncTcpModbusServer } from 'modbus-rs';

// Simulated memory areas
const coils = new Array(1000).fill(false);
const discreteInputs = new Array(1000).fill(false);
const holdingRegisters = new Array(1000).fill(0);
const inputRegisters = new Array(1000).fill(0);

async function main() {
  // Create and bind the server
  const server = await AsyncTcpModbusServer.bind(
    {
      host: '0.0.0.0',
      port: 5502,
    },
    {
      // Read Coils (FC01)
      onReadCoils: (req) => {
        console.log(`Read coils: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return coils.slice(req.address, req.address + req.quantity);
      },

      // Write Single Coil (FC05)
      onWriteSingleCoil: (req) => {
        console.log(`Write single coil: unit=${req.unitId} addr=${req.address} value=${req.value}`);
        coils[req.address] = req.value;
        return true;
      },

      // Write Multiple Coils (FC15)
      onWriteMultipleCoils: (req) => {
        console.log(`Write multiple coils: unit=${req.unitId} addr=${req.address} values=${req.values.length}`);
        for (let i = 0; i < req.values.length; i++) {
          coils[req.address + i] = req.values[i];
        }
        return true;
      },

      // Read Discrete Inputs (FC02)
      onReadDiscreteInputs: (req) => {
        console.log(`Read discrete inputs: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return discreteInputs.slice(req.address, req.address + req.quantity);
      },

      // Read Holding Registers (FC03)
      onReadHoldingRegisters: (req) => {
        console.log(`Read holding registers: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return holdingRegisters.slice(req.address, req.address + req.quantity);
      },

      // Read Input Registers (FC04)
      onReadInputRegisters: (req) => {
        console.log(`Read input registers: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return inputRegisters.slice(req.address, req.address + req.quantity);
      },

      // Write Single Register (FC06)
      onWriteSingleRegister: (req) => {
        console.log(`Write single register: unit=${req.unitId} addr=${req.address} value=${req.value}`);
        holdingRegisters[req.address] = req.value;
        return true;
      },

      // Write Multiple Registers (FC16)
      onWriteMultipleRegisters: (req) => {
        console.log(`Write multiple registers: unit=${req.unitId} addr=${req.address} values=${req.values.length}`);
        for (let i = 0; i < req.values.length; i++) {
          holdingRegisters[req.address + i] = req.values[i];
        }
        return true;
      },
    }
  );

  console.log('Modbus TCP server listening on port 5502');
  console.log('Press Ctrl+C to stop');

  // Keep the server running
  process.on('SIGINT', async () => {
    console.log('\nShutting down server...');
    await server.shutdown();
    process.exit(0);
  });

  // Keep the process alive
  await new Promise(() => {});
}

main().catch(console.error);
