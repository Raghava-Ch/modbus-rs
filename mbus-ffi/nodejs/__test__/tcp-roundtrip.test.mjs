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

test('client fails to connect to non-existent server', async () => {
  // Attempt to connect to a port where no server is listening.
  await assert.rejects(
    async () => {
      await AsyncTcpTransport.connect({
        host: '127.0.0.1',
        port: PORT + 14, // Unused port
        requestTimeoutMs: 500, // Use a short timeout
      });
    },
    (err) => {
      // The exact error message can vary, but it should indicate a connection failure.
      const msg = err.message.toLowerCase();
      assert.ok(
        msg.includes('connection refused') ||
        msg.includes('connect error') ||
        msg.includes('timeout') ||
        msg.includes('connectionfailed'),
        `Unexpected connection error: ${err.message}`
      );
      return true;
    }
  );
});

test('client retries on send failure and succeeds after reconnect', async (t) => {
  // This test simulates a scenario where the server connection is dropped,
  // a client request fails to send, and then the client successfully
  // reconnects and retries the request.

  let server;
  const serverPort = PORT + 16;
  const holdingRegisters = new Uint16Array(10).fill(0);

  // This function creates and starts a server instance.
  const startServer = async () => {
    const s = await AsyncTcpModbusServer.bind(
      { host: '127.0.0.1', port: serverPort, unitId: 1 },
      {
        onWriteSingleRegister: async ({ address, value }) => {
          holdingRegisters[address] = value;
        },
        onReadHoldingRegisters: async ({ address, quantity }) => {
          return Array.from(holdingRegisters.slice(address, address + quantity));
        }
      }
    );
    // Give the server a moment to bind fully.
    await new Promise((r) => setTimeout(r, 50));
    return s;
  };

  server = await startServer();
  t.after(async () => {
    if (server) await server.shutdown();
  });

  // Connect the client with retries enabled.
  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: serverPort,
    requestTimeoutMs: 1000,
    retryAttempts: 3,
    retryDelayMs: 200,
  });
  t.after(async () => await transport.close());

  const client = transport.createClient({ unitId: 1 });

  // 1. Initial successful write to confirm connection.
  await client.writeSingleRegister({ address: 0, value: 42 });
  let regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
  assert.deepEqual(regs, [42], 'Initial write should succeed');

  // 2. Shut down the server to simulate connection loss.
  await server.shutdown();
  server = null;
  await new Promise((r) => setTimeout(r, 100)); // Wait for OS to close port.

  // 3. Attempt a write, which will fail to send and be queued for retry.
  // We don't await this promise yet.
  const writePromise = client.writeSingleRegister({ address: 1, value: 99 });

  // Give the client time to attempt the first send, which should fail.
  await new Promise((r) => setTimeout(r, 100));

  // 4. Restart the server.
  server = await startServer();

  // 5. Now, await the write promise. The client's background task should
  // automatically reconnect and the retry mechanism should successfully
  // send the queued request.
  try {
    await writePromise;
  } catch (err) {
    console.error("WRITE_PROMISE_ERROR:", err);
    throw err;
  }

  // 6. Verify the write succeeded by reading the value back.
  regs = await client.readHoldingRegisters({ address: 1, quantity: 1 });
  assert.deepEqual(regs, [99], 'Write after reconnect and retry should succeed');
});

test('client request fails if server disconnects mid-operation and does not recover', async (t) => {
  // This test validates that if a server disconnects while a client is
  // waiting for a response, the request will eventually fail with a
  // timeout or connection error, even with retries enabled.

  let server;
  server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 17, unitId: 1 },
    {
      onReadHoldingRegisters: async () => {
        // Trigger server shutdown while this handler is running.
        // The client's request will fail because the server closes down.
        // We use a finite timeout longer than the client's outer timeout
        // so no zombie tasks are left behind.
        server.shutdown().catch(() => {});
        await new Promise((r) => setTimeout(r, 2500));
        return [1, 2, 3];
      }
    }
  );
  t.after(async () => {
    if (server) {
      try {
        await server.shutdown();
      } catch (e) { /* ignore */ }
    }
  });

  const transport = await AsyncTcpTransport.connect({ host: '127.0.0.1', port: PORT + 17, requestTimeoutMs: 500, retryAttempts: 2, retryDelayMs: 100 });
  t.after(async () => await transport.close());
  const client = transport.createClient({ unitId: 1 });

  await assert.rejects(client.readHoldingRegisters({ address: 0, quantity: 1 }), (err) => {
    const msg = err.message.toLowerCase();
    assert.ok(msg.includes('timeout') || msg.includes('connection'), `Unexpected error on mid-operation disconnect: ${err.message}`);
    return true;
  });
});

test('server disconnects during client request', async (t) => {
  let server;
  const serverPromise = AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 15, unitId: 1 },
    {
      onReadHoldingRegisters: async () => {
        // This handler will never complete because we shut down the server.
        await new Promise((r) => setTimeout(r, 2000));
        return [1, 2, 3];
      }
    }
  );
  server = await serverPromise;

  // Ensure server is shut down, even if the test fails.
  t.after(async () => {
    if (server) {
      try {
        await server.shutdown();
      } catch (e) { /* ignore, may already be closing */ }
    }
  });
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 15,
    requestTimeoutMs: 1500,
    responseTimeoutMs: 1500,
  });
  t.after(async () => {
    await transport.close();
  });

  const client = transport.createClient({ unitId: 1 });

  // Start a request that will hang on the server side.
  const requestPromise = client.readHoldingRegisters({ address: 0, quantity: 3 });

  // Give the request a moment to be sent to the server.
  await new Promise((r) => setTimeout(r, 100));

  // Now, abruptly shut down the server.
  await server.shutdown();
  server = null; // Prevent the 'after' hook from trying to shut it down again.

  // The pending client request should fail with a connection-related error.
  await assert.rejects(
    requestPromise,
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(
        msg.includes('connection') ||
        msg.includes('workerclosed') ||
        msg.includes('timeout'),
        `Unexpected error on server disconnect: ${err.message}`
      );
      return true;
    }
  );
});

test('high-concurrency client multiplexing', async (t) => {
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: PORT + 18, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        // Add variable delay to verify client maps frames back to their correct transaction IDs
        const delay = Math.floor(Math.random() * 30) + 5;
        await new Promise((r) => setTimeout(r, delay));
        return [req.address, req.quantity];
      }
    }
  );
  t.after(async () => await server.shutdown());
  await new Promise((r) => setTimeout(r, 100));

  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: PORT + 18,
    requestTimeoutMs: 3000,
  });
  t.after(async () => await transport.close());

  const client = transport.createClient({ unitId: 1 });

  const promises = [];
  for (let i = 0; i < 50; i++) {
    promises.push(
      client.readHoldingRegisters({ address: i, quantity: 2 }).then((res) => {
        assert.deepEqual(res, [i, 2], `Mismatch on multiplexed index ${i}`);
      })
    );
  }

  await Promise.all(promises);
});

test('connection fatigue under heavy connect/disconnect cycles', async (t) => {
  const serverPort = PORT + 19;
  const server = await AsyncTcpModbusServer.bind(
    { host: '127.0.0.1', port: serverPort, unitId: 1 },
    {
      onReadHoldingRegisters: async (req) => {
        return [req.address];
      }
    }
  );
  t.after(async () => await server.shutdown());
  await new Promise((r) => setTimeout(r, 100));

  for (let i = 0; i < 10; i++) {
    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: serverPort,
      requestTimeoutMs: 1000,
    });
    const client = transport.createClient({ unitId: 1 });
    const regs = await client.readHoldingRegisters({ address: i, quantity: 1 });
    assert.deepEqual(regs, [i]);
    await transport.close();
  }
});

test('client remains intact and works after manual transport reconnect', async (t) => {
  const serverPort = PORT + 20;
  const holdingRegisters = new Uint16Array(10).fill(0);
  holdingRegisters[0] = 42;
  holdingRegisters[1] = 99;

  const startServer = async () => {
    const s = await AsyncTcpModbusServer.bind(
      { host: '127.0.0.1', port: serverPort, unitId: 1 },
      {
        onReadHoldingRegisters: async ({ address, quantity }) => {
          return Array.from(holdingRegisters.slice(address, address + quantity));
        }
      }
    );
    await new Promise((r) => setTimeout(r, 50));
    return s;
  };

  let server = await startServer();
  t.after(async () => {
    if (server) {
      try { await server.shutdown(); } catch (e) {}
    }
  });

  // Connect client transport with 0 retries so we can control reconnect manually
  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: serverPort,
    requestTimeoutMs: 500,
    retryAttempts: 0,
  });
  t.after(async () => await transport.close());

  const client = transport.createClient({ unitId: 1 });

  // 1. Initial read should succeed
  let regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
  assert.deepEqual(regs, [42], 'Initial read should succeed');

  // 2. Shut down server to break connection
  await server.shutdown();
  server = null;
  await new Promise((r) => setTimeout(r, 100)); // wait for socket cleanup

  // 3. Client read should now fail because server is down
  await assert.rejects(
    client.readHoldingRegisters({ address: 0, quantity: 1 }),
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(msg.includes('connection') || msg.includes('closed'));
      return true;
    },
    'Read should fail when server is shut down'
  );

  // 4. Reconnect should fail because the server is still down
  await assert.rejects(
    transport.reconnect(),
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(msg.includes('refused') || msg.includes('failed') || msg.includes('connection'));
      return true;
    },
    'Reconnect should fail when server is down'
  );

  // 5. Restart the server
  server = await startServer();

  // 6. Reconnect should now succeed
  await transport.reconnect();

  // 7. Reuse the EXACT same client instance to read again — it should work immediately!
  regs = await client.readHoldingRegisters({ address: 1, quantity: 1 });
  assert.deepEqual(regs, [99], 'Read after reconnect should succeed using original client');
});


test('client remains intact and fails after manual transport reconnect', async (t) => {
  const serverPort = PORT + 21;
  const holdingRegisters = new Uint16Array(10).fill(0);
  holdingRegisters[0] = 42;
  holdingRegisters[1] = 99;

  const startServer = async () => {
    const s = await AsyncTcpModbusServer.bind(
      { host: '127.0.0.1', port: serverPort, unitId: 1 },
      {
        onReadHoldingRegisters: async ({ address, quantity }) => {
          return Array.from(holdingRegisters.slice(address, address + quantity));
        }
      }
    );
    await new Promise((r) => setTimeout(r, 50));
    return s;
  };

  let server = await startServer();
  t.after(async () => {
    if (server) {
      try { await server.shutdown(); } catch (e) {}
    }
  });

  // Connect client transport with 0 retries so we can control reconnect manually
  const transport = await AsyncTcpTransport.connect({
    host: '127.0.0.1',
    port: serverPort,
    requestTimeoutMs: 500,
    retryAttempts: 0,
  });
  t.after(async () => await transport.close());

  const client = transport.createClient({ unitId: 1 });

  // 1. Initial read should succeed
  let regs = await client.readHoldingRegisters({ address: 0, quantity: 1 });
  assert.deepEqual(regs, [42], 'Initial read should succeed');

  // 2. Shut down server to break connection
  await server.shutdown();
  server = null;
  await new Promise((r) => setTimeout(r, 100)); // wait for socket cleanup

  // 3. Client read should now fail because server is down
  await assert.rejects(
    client.readHoldingRegisters({ address: 0, quantity: 1 }),
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(msg.includes('connection') || msg.includes('closed'));
      return true;
    },
    'Read should fail when server is shut down'
  );

  // 4. Reconnect should fail because the server is still down
  await assert.rejects(
    transport.reconnect(),
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(msg.includes('refused') || msg.includes('failed') || msg.includes('connection'));
      return true;
    },
    'Reconnect should fail when server is down'
  );

  // Server never started again

  // 5. Reuse the EXACT same client instance to read again — it should fail immediately!
  await assert.rejects(
    client.readHoldingRegisters({ address: 0, quantity: 1 }),
    (err) => {
      const msg = err.message.toLowerCase();
      assert.ok(msg.includes('connection') || msg.includes('closed'));
      return true;
    },
    'Read should fail when server is shut down'
  );
});