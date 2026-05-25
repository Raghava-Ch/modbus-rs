// Integration test: spin up the server class, connect the client, and
// exercise the operations that the v0.8 binding actually supports.
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

test('round-trip reads/writes with JS server handlers', async (t) => {
  // Setup in-memory databases on the JS side
  const holdingRegisters = new Uint16Array(100);
  const coils = new Uint8Array(100);

  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 3, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        const { address, quantity } = req;
        const vals = Array.from(holdingRegisters.slice(address, address + quantity));
        return vals;
      },
      onWriteSingleRegister: async (req) => {
        const { address, value } = req;
        holdingRegisters[address] = value;
      },
      onWriteMultipleRegisters: async (req) => {
        const { address, values } = req;
        for (let i = 0; i < values.length; i++) {
          holdingRegisters[address + i] = values[i];
        }
      },
      onReadCoils: async (req) => {
        const { address, quantity } = req;
        const vals = Array.from(coils.slice(address, address + quantity)).map(v => v === 1);
        return vals;
      },
      onWriteSingleCoil: async (req) => {
        const { address, value } = req;
        coils[address] = value ? 1 : 0;
      },
    }
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: PORT + 3,
    unitId: 1,
    timeoutMs: 2000,
  });
  t.after(async () => {
    await client.close();
  });

  // Test holding registers round-trip
  await client.writeSingleRegister({ address: 10, value: 1234 });
  let regs = await client.readHoldingRegisters({ address: 10, quantity: 1 });
  assert.deepEqual(regs, [1234]);

  await client.writeMultipleRegisters({ address: 20, values: [5, 6, 7] });
  regs = await client.readHoldingRegisters({ address: 20, quantity: 3 });
  assert.deepEqual(regs, [5, 6, 7]);

  // Test coils round-trip
  await client.writeSingleCoil({ address: 5, value: true });
  let coilsVal = await client.readCoils({ address: 5, quantity: 1 });
  assert.deepEqual(coilsVal, [true]);
});

test('server exception handling: returning custom error exceptions', async (t) => {
  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 4, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        if (req.address >= 50) {
          // Return Modbus exception 2 (Illegal Data Address)
          return { exception: 2 };
        }
        return [0];
      }
    }
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: PORT + 4,
    unitId: 1,
    timeoutMs: 2000,
  });
  t.after(async () => {
    await client.close();
  });

  // Valid address should succeed
  const ok = await client.readHoldingRegisters({ address: 10, quantity: 1 });
  assert.deepEqual(ok, [0]);

  // Invalid address should fail with Modbus Exception 2
  await assert.rejects(
    async () => {
      await client.readHoldingRegisters({ address: 55, quantity: 1 });
    },
    (err) => {
      assert.ok(err.message.includes('exception code: 2') || err.message.includes('MODBUS_EXCEPTION:2'), `Unexpected error message: ${err.message}`);
      return true;
    }
  );
});

test('per-request client AbortSignal cancellation', async (t) => {
  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 5, unitId: 1 },
    {}
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: PORT + 5,
    unitId: 1,
    timeoutMs: 2000,
  });
  t.after(async () => {
    await client.close();
  });

  const controller = new AbortController();
  const promise = client.readHoldingRegisters({
    address: 0,
    quantity: 1,
    signal: controller.signal,
  });

  // Abort immediately
  controller.abort();

  await assert.rejects(
    promise,
    (err) => {
      assert.ok(err.message.includes('aborted') || err.code === 'Cancelled', `Unexpected abort error: ${err.message}`);
      return true;
    }
  );
});

test('round-trip with sync (non-async) handlers', async (t) => {
  const holdingRegisters = new Array(100).fill(0);

  const server = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 6, unitId: 1 },
    {
      // Intentionally NOT async
      onReadHoldingRegisters: (req) => {
        return holdingRegisters.slice(req.address, req.address + req.quantity);
      },
      onWriteSingleRegister: (req) => {
        holdingRegisters[req.address] = req.value;
      },
    }
  );
  t.after(async () => { await server.shutdown(); });
  await new Promise((r) => setTimeout(r, 100));

  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1', port: PORT + 6, unitId: 1, timeoutMs: 2000,
  });
  t.after(async () => { await client.close(); });

  await client.writeSingleRegister({ address: 0, value: 42 });
  const regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
  assert.deepEqual(regs, [42]);
});
