/**
 * Serial RTU Client Example
 * 
 * Demonstrates connecting to a Modbus RTU device over serial.
 */

const { AsyncSerialModbusClient } = require('modbus-rs');

async function main() {
  // Connect to a Modbus RTU device
  const client = await AsyncSerialModbusClient.connectRtu({
    portPath: '/dev/ttyUSB0',  // or 'COM1' on Windows
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
    // Read holding registers
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Holding registers:', registers);

    // Read coils
    const coils = await client.readCoils({
      address: 0,
      quantity: 8,
    });
    console.log('Coils:', coils);

    // Write single register
    await client.writeSingleRegister({
      address: 0,
      value: 12345,
    });
    console.log('Wrote register');

  } catch (err) {
    console.error('Error:', err.message);
  } finally {
    await client.close();
  }
}

main().catch(console.error);
