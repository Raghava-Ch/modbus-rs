// Integration test: spin up the server class, connect the client, and
// exercise the operations that the v0.8 binding actually supports.
//
// **KNOWN LIMITATION (v0.8):** the server's JS handler-callback dispatch
// is not yet wired up — the server stub echoes write requests but
// always returns IllegalFunction for read requests.  Once the
// ThreadsafeFunction-based dispatch is implemented, this suite will be
// expanded to cover round-trip reads from the JS handlers.
//
// Run with:  npm test    (which is `node --test __test__/`)

import { test } from 'node:test';
import assert from 'node:assert/strict';

let modbus;
try {
  modbus = await import('../index.js');
} catch (err) {
  console.warn(
    `[skip] Native modbus-rs addon not loadable — did you run 'npm run build'?`,
    `\n      Reason: ${err.message}`,
  );
  test('native addon present', { skip: true }, () => {});
  // eslint-disable-next-line no-process-exit
  process.exit(0);
}

const { AsyncTcpModbusClient, AsyncTcpModbusServer } = modbus;

const PORT = 25502;

test('server lifecycle: bind / shutdown', async () => {
  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT, unitId: 1 },
    {}, // empty handler bag
  );
  assert.ok(server, 'bind() should return a server instance');
  await server.shutdown();
});

test('client lifecycle: connect / close against stub server', async (t) => {
  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 1, unitId: 1 },
    {},
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: PORT + 1,
    unitId: 1,
    timeoutMs: 2000,
  });
  assert.ok(client, 'connect() should return a client instance');

  // The stub server echoes write requests, so a write should succeed.
  await client.writeSingleRegister({ address: 0, value: 42 });
  await client.writeMultipleRegisters({
    address: 10,
    values: [1, 2, 3, 4],
  });

  await client.close();
});

test('client surface: all expected methods exist', async (t) => {
  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 2, unitId: 1 },
    {},
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: PORT + 2,
    unitId: 1,
    timeoutMs: 2000,
  });
  t.after(async () => {
    await client.close();
  });

  for (const method of [
    'readHoldingRegisters',
    'readInputRegisters',
    'writeSingleRegister',
    'writeMultipleRegisters',
    'readCoils',
    'readDiscreteInputs',
    'writeSingleCoil',
    'writeMultipleCoils',
    'close',
  ]) {
    assert.equal(
      typeof client[method],
      'function',
      `client should expose ${method}()`,
    );
  }
});
