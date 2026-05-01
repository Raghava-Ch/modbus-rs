/**
 * Example 03 — TCP client: coils (FC01/05/15) and discrete inputs (FC02)
 *
 * Demonstrates the boolean-valued function codes.  Boolean values are
 * passed and returned as plain JS `boolean[]` arrays.
 *
 * Run:    node examples/03-tcp-client-coils-and-discrete-inputs.mjs
 */

import { AsyncTcpModbusClient } from 'modbus-rs';

const HOST = process.env.MODBUS_HOST ?? '127.0.0.1';
const PORT = Number(process.env.MODBUS_PORT ?? 5502);

async function main() {
  const client = await AsyncTcpModbusClient.connect({
    host: HOST,
    port: PORT,
    unitId: 1,
    timeoutMs: 2000,
  });

  try {
    // FC05 — Write Single Coil
    await client.writeSingleCoil({ address: 0, value: true });
    console.log('FC05 wrote coil[0] = true');

    // FC15 — Write Multiple Coils
    const coilPattern = [true, false, true, true, false, false, true, false];
    await client.writeMultipleCoils({ address: 10, values: coilPattern });
    console.log('FC15 wrote coils[10..18]:', coilPattern);

    // FC01 — Read Coils
    const coils = await client.readCoils({ address: 0, quantity: 20 });
    console.log('FC01 coils[0..20]:', coils);

    // FC02 — Read Discrete Inputs
    const dis = await client.readDiscreteInputs({ address: 0, quantity: 8 });
    console.log('FC02 discrete inputs[0..8]:', dis);
  } finally {
    await client.close();
  }
}

main().catch((err) => {
  console.error('Fatal:', err.message);
  process.exit(1);
});
