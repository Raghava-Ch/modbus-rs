import { test } from 'node:test';
import assert from 'node:assert/strict';

let modbus;
try {
  modbus = await import('../../dist/index.js');
} catch {
  test('serial port path tests (skipped - native addon not built)', { skip: true }, () => {});
}

if (modbus) {
  const { AsyncRtuTransport, AsyncAsciiTransport, AsyncSerialModbusServer } = modbus;

  test('AsyncRtuTransport.open rejects when portPath exceeds limit', async () => {
    // Generate a port path longer than any supported limit
    const overlyLongPort = '/dev/' + 'a'.repeat(256);

    await assert.rejects(
      async () => {
        await AsyncRtuTransport.open({
          portPath: overlyLongPort,
          baudRate: 9600,
        });
      },
      (err) => {
        assert.equal(err.code, 'InvalidArg');
        assert.match(err.message, /Port path too long/i);
        return true;
      },
    );
  });

  test('AsyncAsciiTransport.open rejects when portPath exceeds limit', async () => {
    const overlyLongPort = '/dev/' + 'a'.repeat(256);

    await assert.rejects(
      async () => {
        await AsyncAsciiTransport.open({
          portPath: overlyLongPort,
          baudRate: 9600,
        });
      },
      (err) => {
        assert.equal(err.code, 'InvalidArg');
        assert.match(err.message, /Port path too long/i);
        return true;
      },
    );
  });

  test('AsyncSerialModbusServer.bindRtu throws when portPath exceeds limit', () => {
    const overlyLongPort = '/dev/' + 'a'.repeat(256);

    assert.throws(
      () => {
        AsyncSerialModbusServer.bindRtu(
          {
            portPath: overlyLongPort,
            baudRate: 9600,
            unitId: 1,
          },
          {},
        );
      },
      (err) => {
        assert.equal(err.code, 'InvalidArg');
        assert.match(err.message, /Port path too long/i);
        return true;
      },
    );
  });
}
