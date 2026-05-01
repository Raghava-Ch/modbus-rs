/**
 * TCP Client Example
 * 
 * Demonstrates connecting to a Modbus TCP server and reading registers.
 */

const { AsyncTcpModbusClient } = require('modbus-rs');

async function main() {
  // Connect to a Modbus TCP server
  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: 502,
    unitId: 1,
    timeoutMs: 5000,
  });

  try {
    // Read holding registers (FC03)
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Holding registers:', registers);

    // Read input registers (FC04)
    const inputs = await client.readInputRegisters({
      address: 0,
      quantity: 5,
    });
    console.log('Input registers:', inputs);

    // Read coils (FC01)
    const coils = await client.readCoils({
      address: 0,
      quantity: 16,
    });
    console.log('Coils:', coils);

    // Write single register (FC06)
    await client.writeSingleRegister({
      address: 10,
      value: 12345,
    });
    console.log('Wrote single register');

    // Write multiple registers (FC16)
    await client.writeMultipleRegisters({
      address: 0,
      values: [100, 200, 300, 400, 500],
    });
    console.log('Wrote multiple registers');

    // Write single coil (FC05)
    await client.writeSingleCoil({
      address: 0,
      value: true,
    });
    console.log('Wrote single coil');

    // Write multiple coils (FC15)
    await client.writeMultipleCoils({
      address: 0,
      values: [true, false, true, true, false, false, true, false],
    });
    console.log('Wrote multiple coils');

  } catch (err) {
    console.error('Error:', err.message);
  } finally {
    // Close the connection
    await client.close();
  }
}

main().catch(console.error);
