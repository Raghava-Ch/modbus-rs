/**
 * Example 09 — Serial RTU client
 *
 * ───────────────────────────────────────────────────────────────────────────
 * REQUIREMENTS
 * ───────────────────────────────────────────────────────────────────────────
 *  You need a serial port to run this example.  Pick one of:
 *
 *    1) A USB-to-RS485 dongle (e.g. CH340, FT232) connected to a real
 *       Modbus RTU device or PLC.
 *
 *    2) A virtual serial-port pair plus a software simulator:
 *         Linux/macOS:
 *           # In one terminal create the pair:
 *           socat -d -d PTY,raw,echo=0,link=/tmp/ttyV0 \
 *                       PTY,raw,echo=0,link=/tmp/ttyV1
 *           # Then run a simulator on one end:
 *           diagslave -m rtu -a 1 -b 19200 -p even /tmp/ttyV1
 *           # And point this script at the other end (/tmp/ttyV0):
 *           PORT=/tmp/ttyV0 node examples/09-serial-rtu-client.mjs
 *
 *         Windows:
 *           Use `com0com` (https://com0com.sourceforge.net/) to create a
 *           virtual COM-port pair (e.g. COM3 ↔ COM4), and run a simulator
 *           such as `Modbus Slave` (https://www.modbustools.com/) or
 *           `modbus-lab` on one side.
 *
 *    3) An online simulator like `modbus-lab` or `pyModSlave`.
 *
 *  Set `PORT` env-var to your port path, e.g.
 *    /dev/ttyUSB0           Linux
 *    /dev/cu.usbserial-XXXX macOS
 *    COM3                   Windows
 * ───────────────────────────────────────────────────────────────────────────
 *
 * Run:    PORT=/dev/ttyUSB0 node examples/09-serial-rtu-client.mjs
 */

import { AsyncSerialModbusClient } from 'modbus-rs';

const PORT = process.env.PORT ?? '/dev/ttyUSB0';

async function main() {
  const client = await AsyncSerialModbusClient.connectRtu({
    portPath: PORT,
    unitId: 1,
    baudRate: 19200,
    dataBits: 8,
    stopBits: 1,
    parity: 'even',
    requestTimeoutMs: 1000,
    retryAttempts: 3,
    backoffStrategy: 'exponential',
    backoffDelayMs: 100,
  });

  try {
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Holding registers:', registers);

    const coils = await client.readCoils({ address: 0, quantity: 8 });
    console.log('Coils:', coils);

    await client.writeSingleRegister({ address: 0, value: 12345 });
    console.log('Wrote register');
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});

