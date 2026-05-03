/**
 * Example 11 — Serial ASCII client
 *
 * ───────────────────────────────────────────────────────────────────────────
 * REQUIREMENTS — same as Example 09 (see its header for full setup notes).
 * Use a simulator or device that speaks Modbus ASCII (7 data bits, even
 * parity is the most common configuration).
 *
 *   diagslave -m ascii -a 1 -b 19200 -p even /tmp/ttyV1
 * ───────────────────────────────────────────────────────────────────────────
 *
 * Run:    PORT=/dev/ttyUSB0 node examples/11-serial-ascii-client.mjs
 */

import { AsyncSerialModbusClient } from 'modbus-rs';

const PORT = process.env.PORT ?? '/dev/ttyUSB0';

async function main() {
  const client = await AsyncSerialModbusClient.connectAscii({
    portPath: PORT,
    unitId: 1,
    baudRate: 19200,
    dataBits: 7,    // ASCII typically uses 7 data bits
    stopBits: 1,
    parity: 'even',
    requestTimeoutMs: 1500,
  });

  try {
    const regs = await client.readHoldingRegisters({ address: 0, quantity: 4 });
    console.log('Holding registers (ASCII):', regs);

    await client.writeMultipleRegisters({
      address: 0,
      values: [0xAA55, 0x1234, 0x5678, 0x9ABC],
    });
    console.log('Wrote 4 registers');
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
