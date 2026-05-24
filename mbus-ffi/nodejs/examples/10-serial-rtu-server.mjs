/**
 * Example 10 — Serial RTU server
 *
 * ───────────────────────────────────────────────────────────────────────────
 * REQUIREMENTS
 * ───────────────────────────────────────────────────────────────────────────
 *  You need a serial port to run this example. Pick one of:
 *
 *    1) A USB-to-RS485 dongle (e.g. CH340, FT232) connected to a real
 *       Modbus RTU device or PLC.
 *
 *    2) A virtual serial-port pair:
 *         Linux/macOS:
 *           # Create the pair:
 *           socat -d -d PTY,raw,echo=0,link=/tmp/ttyV0 \
 *                       PTY,raw,echo=0,link=/tmp/ttyV1
 *           # Run this server on /tmp/ttyV1:
 *           PORT=/tmp/ttyV1 node examples/10-serial-rtu-server.mjs
 *           # Run a client on the other side (/tmp/ttyV0):
 *           PORT=/tmp/ttyV0 node examples/09-serial-rtu-client.mjs
 *
 *         Windows:
 *           Use `com0com` (https://com0com.sourceforge.net/) to create a
 *           virtual COM-port pair (e.g. COM3 ↔ COM4), and run this server on
 *           COM4 and client on COM3.
 *
 *  Set `PORT` env-var to your port path, e.g.
 *    /dev/ttyUSB0           Linux
 *    /dev/cu.usbserial-XXXX macOS
 *    COM4                   Windows
 * ───────────────────────────────────────────────────────────────────────────
 *
 * Run:    PORT=/dev/ttyUSB0 node examples/10-serial-rtu-server.mjs
 */

import { AsyncSerialModbusServer } from 'modbus-rs';

const PORT = process.env.PORT ?? '/dev/ttyUSB1';

// Simulated memory areas
const coils = new Array(100).fill(false);
const discreteInputs = new Array(100).fill(false);
const holdingRegisters = new Array(100).fill(0);
const inputRegisters = new Array(100).fill(0);

// Initialize some registers with realistic test data
holdingRegisters[0] = 42;
holdingRegisters[1] = 100;
holdingRegisters[2] = 200;
holdingRegisters[3] = 300;
inputRegisters[0] = 999;
coils[0] = true;
coils[2] = true;
discreteInputs[0] = true;
discreteInputs[1] = true;

async function main() {
  console.log(`Starting Serial RTU Server on port: ${PORT}`);

  // Binds and starts the Serial RTU server
  const server = await AsyncSerialModbusServer.bindRtu(
    {
      portPath: PORT,
      baudRate: 19200,
      parity: 'even',
      dataBits: 8,
      stopBits: 1,
      unitId: 1,
    },
    {
      // Read Coils (FC01)
      onReadCoils: (req) => {
        console.log(`[Serial Server] Read coils: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return coils.slice(req.address, req.address + req.quantity);
      },

      // Write Single Coil (FC05)
      onWriteSingleCoil: (req) => {
        console.log(`[Serial Server] Write single coil: unit=${req.unitId} addr=${req.address} value=${req.value}`);
        coils[req.address] = req.value;
        return true;
      },

      // Write Multiple Coils (FC15)
      onWriteMultipleCoils: (req) => {
        console.log(`[Serial Server] Write multiple coils: unit=${req.unitId} addr=${req.address} values=${req.values.length}`);
        for (let i = 0; i < req.values.length; i++) {
          coils[req.address + i] = req.values[i];
        }
        return true;
      },

      // Read Discrete Inputs (FC02)
      onReadDiscreteInputs: (req) => {
        console.log(`[Serial Server] Read discrete inputs: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return discreteInputs.slice(req.address, req.address + req.quantity);
      },

      // Read Holding Registers (FC03)
      onReadHoldingRegisters: (req) => {
        console.log(`[Serial Server] Read holding registers: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        if (req.address + req.quantity > holdingRegisters.length) {
          return { exception: 2 }; // Illegal Data Address
        }
        return holdingRegisters.slice(req.address, req.address + req.quantity);
      },

      // Read Input Registers (FC04)
      onReadInputRegisters: (req) => {
        console.log(`[Serial Server] Read input registers: unit=${req.unitId} addr=${req.address} quantity=${req.quantity}`);
        return inputRegisters.slice(req.address, req.address + req.quantity);
      },

      // Write Single Register (FC06)
      onWriteSingleRegister: (req) => {
        console.log(`[Serial Server] Write single register: unit=${req.unitId} addr=${req.address} value=${req.value}`);
        holdingRegisters[req.address] = req.value;
        return true;
      },

      // Write Multiple Registers (FC16)
      onWriteMultipleRegisters: (req) => {
        console.log(`[Serial Server] Write multiple registers: unit=${req.unitId} addr=${req.address} values=${req.values.length}`);
        for (let i = 0; i < req.values.length; i++) {
          holdingRegisters[req.address + i] = req.values[i];
        }
        return true;
      },
    }
  );

  console.log(`Modbus Serial RTU server listening on ${PORT} (19200, 8E1)`);
  console.log('Press Ctrl+C to stop');

  // Handle graceful shutdown
  process.on('SIGINT', async () => {
    console.log('\nShutting down Serial RTU server...');
    await server.shutdown();
    console.log('Server stopped.');
    process.exit(0);
  });

  // Keep the process alive
  await new Promise(() => {});
}

main().catch((err) => {
  console.error('Fatal error starting Serial RTU server:', err);
  process.exit(1);
});
