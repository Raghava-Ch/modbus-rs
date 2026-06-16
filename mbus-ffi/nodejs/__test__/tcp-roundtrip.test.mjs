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

const { AsyncTcpTransport, AsyncTcpModbusServer, AsyncTcpGateway } = modbus;

const PORT = 25502;

test('server lifecycle: bind / shutdown', async () => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT, unitId: 1 },
    {}, // empty handler bag
  );
  assert.ok(server, 'bind() should return a server instance');
  await server.shutdown();
});

test('client lifecycle: connect / close against stub server', async (t) => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 1, unitId: 1 },
    {},
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 1,
    requestTimeoutMs: 2000,
  });
  assert.ok(transport, 'connect() should return a transport instance');

  const client = transport.createClient({ unitId: 1 });
  assert.ok(client, 'createClient() should return a client instance');

  // The stub server echoes write requests, so a write should succeed.
  await client.writeSingleRegister({ address: 0, value: 42 });
  await client.writeMultipleRegisters({
    address: 10,
    values: [1, 2, 3, 4],
  });

  await transport.close();
});

test('client surface: all expected methods exist', async (t) => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 2, unitId: 1 },
    {},
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 2,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

  for (const method of [
    'readHoldingRegisters',
    'readInputRegisters',
    'writeSingleRegister',
    'writeMultipleRegisters',
    'readCoils',
    'readDiscreteInputs',
    'writeSingleCoil',
    'writeMultipleCoils',
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

  const server = await AsyncTcpModbusServer.bind(
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

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 3,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

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
  const server = await AsyncTcpModbusServer.bind(
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

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 4,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

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
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 5, unitId: 1 },
    {}
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 5,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

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

  const server = await AsyncTcpModbusServer.bind(
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

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1', port: PORT + 6, requestTimeoutMs: 2000,
  });
  t.after(async () => { await transport.close(); });

  const client = transport.createClient({ unitId: 1 });

  await client.writeSingleRegister({ address: 0, value: 42 });
  const regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
  assert.deepEqual(regs, [42]);
});

test('multi-drop TCP client sharing (same unit ID)', async (t) => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 7, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        return [100 + req.address];
      },
    }
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 7,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client1 = transport.createClient({ unitId: 1 });
  const client2 = transport.createClient({ unitId: 1 });

  const res1 = await client1.readHoldingRegisters({ address: 5, quantity: 1 });
  const res2 = await client2.readHoldingRegisters({ address: 10, quantity: 1 });

  assert.deepEqual(res1, [105]);
  assert.deepEqual(res2, [110]);
});

test('logical client lifecycle on transport close', async (t) => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 8, unitId: 1 },
    {}
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 8,
    requestTimeoutMs: 2000,
  });
  const client = transport.createClient({ unitId: 1 });

  await transport.close();

  await assert.rejects(
    async () => {
      await client.readHoldingRegisters({ address: 0, quantity: 1 });
    },
    (err) => {
      assert.ok(err.message.includes('closed') || err.message.includes('WorkerClosed') || err.message.includes('connection'), `Unexpected close error: ${err.message}`);
      return true;
    }
  );
});

test('gateway routing (different unit IDs) over single transport', async (t) => {
  // Bind backend server for unit 10
  const server10 = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 9, unitId: 10 },
    {
      onReadHoldingRegisters: async (req) => {
        return [1000 + req.address];
      },
    }
  );
  t.after(async () => {
    await server10.shutdown();
  });

  // Bind backend server for unit 20
  const server20 = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 10, unitId: 20 },
    {
      onReadHoldingRegisters: async (req) => {
        return [2000 + req.address];
      },
    }
  );
  t.after(async () => {
    await server20.shutdown();
  });

  await new Promise((r) => setTimeout(r, 100));

  // Bind gateway to route unit 10 -> server10, unit 20 -> server20
  const gateway = await AsyncTcpGateway.bind(
    { host: '127.0.0.1', port: PORT + 11 },
    {
      downstreams: [
        { host: '127.0.0.1', port: PORT + 9 },
        { host: '127.0.0.1', port: PORT + 10 },
      ],
      routes: [
        { unitId: 10, channel: 0 },
        { unitId: 20, channel: 1 },
      ],
    }
  );
  t.after(async () => {
    await gateway.shutdown();
  });

  await new Promise((r) => setTimeout(r, 100));

  // Connect client transport to gateway
  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 11,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  // Create clients for units 10 and 20 from same transport connection
  const client10 = transport.createClient({ unitId: 10 });
  const client20 = transport.createClient({ unitId: 20 });

  // Query both and assert they routed to the correct backend server
  const res10 = await client10.readHoldingRegisters({ address: 5, quantity: 1 });
  const res20 = await client20.readHoldingRegisters({ address: 5, quantity: 1 });

  assert.deepEqual(res10, [1005]);
  assert.deepEqual(res20, [2005]);
});

test('ModbusErrorCode constants exist and are strings', () => {
  assert.equal(typeof modbus.ModbusErrorCode.EXCEPTION, 'string');
  assert.equal(modbus.ModbusErrorCode.EXCEPTION, 'MODBUS_EXCEPTION');
  assert.equal(modbus.ModbusErrorCode.TIMEOUT, 'MODBUS_TIMEOUT');
});

test('getModbusErrorCode extracts correct code', () => {
  const err = new Error('[MODBUS_EXCEPTION:3] Modbus exception code: 3');
  assert.equal(modbus.getModbusErrorCode(err), 'MODBUS_EXCEPTION');
  assert.equal(modbus.getModbusErrorCode(new Error('some random error')), undefined);
});

test('client requestTimeoutMs works', async (t) => {
  // Bind server that delays response to trigger timeout
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 12, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        await new Promise((r) => setTimeout(r, 1000));
        return [1, 2, 3];
      }
    }
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 12,
    requestTimeoutMs: 200, // short timeout
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });
  await assert.rejects(
    async () => {
      await client.readHoldingRegisters({ address: 0, quantity: 3 });
    },
    (err) => {
      const code = modbus.getModbusErrorCode(err);
      assert.equal(code, modbus.ModbusErrorCode.TIMEOUT);
      return true;
    }
  );
});

test('round-trip reads/writes for remaining Modbus services (discrete inputs, input registers, read/write multiple registers, FIFO queue, diagnostics, file records, exception status)', async (t) => {
  const fileRecords = {
    // fileNumber -> recordNumber -> recordData
    4: { 1: [10, 20, 30] }
  };

  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 13, unitId: 1 },
    {
      onReadDiscreteInputs: async (req) => {
        const { address, quantity } = req;
        const result = [];
        for (let i = 0; i < quantity; i++) {
          result.push((address + i) % 2 === 0);
        }
        return result;
      },
      onReadInputRegisters: async (req) => {
        const { address, quantity } = req;
        const result = [];
        for (let i = 0; i < quantity; i++) {
          result.push(address + i + 100);
        }
        return result;
      },
      onReadWriteMultipleRegisters: async (req) => {
        const { readAddress, readQuantity, writeAddress, writeValues } = req;
        // Just echo back values based on readAddress for this test
        const result = [];
        for (let i = 0; i < readQuantity; i++) {
          result.push(readAddress + i + writeValues[0]);
        }
        return result;
      },
      onReadFifoQueue: async (req) => {
        const { address } = req;
        return [address, address + 1, address + 2];
      },
      onReadExceptionStatus: async () => {
        return 0x5A;
      },
      onDiagnostics: async (req) => {
        const { subFunction, data } = req;
        return {
          subFunction: subFunction,
          data: data.map(v => v + 1)
        };
      },
      onReadFileRecord: async (req) => {
        const { requests } = req;
        const responses = [];
        for (const sub of requests) {
          const file = fileRecords[sub.fileNumber];
          const rec = file ? file[sub.recordNumber] : undefined;
          if (rec) {
            responses.push(rec.slice(0, sub.recordLength));
          } else {
            responses.push(new Array(sub.recordLength).fill(0));
          }
        }
        return responses;
      },
      onWriteFileRecord: async (req) => {
        const { requests } = req;
        for (const sub of requests) {
          if (!fileRecords[sub.fileNumber]) {
            fileRecords[sub.fileNumber] = {};
          }
          fileRecords[sub.fileNumber][sub.recordNumber] = sub.recordData;
        }
      }
    }
  );
  t.after(async () => {
    await server.shutdown();
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 13,
    requestTimeoutMs: 2000,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

  // 1. Read Discrete Inputs
  const discreteRes = await client.readDiscreteInputs({ address: 10, quantity: 4 });
  assert.deepEqual(discreteRes, [true, false, true, false]);

  // 2. Read Input Registers
  const inputRegsRes = await client.readInputRegisters({ address: 20, quantity: 2 });
  assert.deepEqual(inputRegsRes, [120, 121]);

  // 3. Read/Write Multiple Registers
  const rwRegsRes = await client.readWriteMultipleRegisters({
    readAddress: 30,
    readQuantity: 2,
    writeAddress: 40,
    writeValues: [10]
  });
  assert.deepEqual(rwRegsRes, [40, 41]);

  // 4. Read FIFO Queue
  const fifoRes = await client.readFifoQueue({ address: 50 });
  assert.deepEqual(fifoRes.values, [50, 51, 52]);

  // 5. Read Exception Status
  const excStatus = await client.readExceptionStatus();
  assert.equal(excStatus, 0x5A);

  // 6. Diagnostics
  const diagRes = await client.diagnostics({ subFunction: 3, data: [100] });
  assert.equal(diagRes.subFunction, 3);
  assert.deepEqual(diagRes.data, [101]);

  // 7. Write File Record & Read File Record
  await client.writeFileRecord({
    requests: [
      { fileNumber: 4, recordNumber: 2, recordData: [90, 80] }
    ]
  });

  const fileRes = await client.readFileRecord({
    requests: [
      { fileNumber: 4, recordNumber: 1, recordLength: 3 },
      { fileNumber: 4, recordNumber: 2, recordLength: 2 }
    ]
  });
  assert.deepEqual(fileRes, [
    [10, 20, 30],
    [90, 80]
  ]);
});

