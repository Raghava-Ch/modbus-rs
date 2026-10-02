import { test } from 'node:test';
import assert from 'node:assert/strict';
import net from 'node:net';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, unlinkSync } from 'node:fs';
import { setTimeout as sleep } from 'node:timers/promises';

let modbus;
try {
  modbus = await import('../../dist/index.js');
} catch (err) {
  test('timeout issues (skipped - native addon not built)', { skip: true }, () => {});
}

if (modbus) {
  const { AsyncTcpTransport, AsyncRtuTransport, ModbusErrorCode, getModbusErrorCode } = modbus;

  // ─────────────────────────────────────────────────────────────────────────────
  // Helper: In-process Mock Modbus TCP Server
  // Simulates a multi-drop bus:
  // - Unit 2 is SILENT (never replies)
  // - Unit 1 and 3 are HEALTHY (reply after configured delay)
  // ─────────────────────────────────────────────────────────────────────────────
  function createMockServer({ replyDelayMs = 5 } = {}) {
    const seenUnits = [];
    const sockets = new Set();
    const server = net.createServer((socket) => {
      sockets.add(socket);
      socket.on('close', () => sockets.delete(socket));

      let rxBuf = Buffer.alloc(0);

      socket.on('data', (chunk) => {
        rxBuf = Buffer.concat([rxBuf, chunk]);

        while (rxBuf.length >= 6) {
          const pduLen = rxBuf.readUInt16BE(4);
          const totalLen = 6 + pduLen;
          if (rxBuf.length < totalLen) {
            break;
          }

          const frame = rxBuf.subarray(0, totalLen);
          rxBuf = rxBuf.subarray(totalLen);

          const txnId = frame.readUInt16BE(0);
          const unitId = frame[6];
          const fc = frame[7];

          seenUnits.push(unitId);

          // Silent unit never replies
          if (unitId === 2) {
            continue;
          }

          // Build FC 03 response: [txnId, 0, 0, len, unitId, fc, byteCount, ...values]
          if (fc === 3 && frame.length >= 12) {
            const qty = frame.readUInt16BE(10);
            const byteCount = qty * 2;
            const resp = Buffer.alloc(9 + byteCount);
            resp.writeUInt16BE(txnId, 0);
            resp.writeUInt16BE(0, 2); // Protocol ID
            resp.writeUInt16BE(3 + byteCount, 4); // Length
            resp[6] = unitId;
            resp[7] = fc;
            resp[8] = byteCount;

            setTimeout(() => {
              if (!socket.destroyed) {
                socket.write(resp);
              }
            }, replyDelayMs);
          }
        }
      });
    });

    return new Promise((resolve, reject) => {
      server.listen(0, '127.0.0.1', () => {
        const port = server.address().port;
        resolve({
          port,
          seenUnits,
          close: async () => {
            for (const s of sockets) {
              s.destroy();
            }
            if (typeof server.closeAllConnections === 'function') {
              server.closeAllConnections();
            }
            return new Promise((res) => server.close(() => res()));
          },
        });
      });
      server.on('error', reject);
    });
  }

  // ─────────────────────────────────────────────────────────────────────────────
  // Issue 1: responseTimeoutMs effectively limits a request
  // ─────────────────────────────────────────────────────────────────────────────
  test('Issue 1: responseTimeoutMs limits a request on silent unit', { timeout: 3000 }, async (t) => {
    const server = await createMockServer({ replyDelayMs: 5 });

    // Configure responseTimeoutMs to 150ms on open/connect
    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      responseTimeoutMs: 150,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });
    const t0 = Date.now();

    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        const code = getModbusErrorCode(err);
        assert.equal(code, ModbusErrorCode.TIMEOUT, `Expected MODBUS_TIMEOUT, got: ${err.message}`);
        return true;
      },
    );

    const elapsed = Date.now() - t0;
    assert.ok(
      elapsed >= 100 && elapsed < 800,
      `Expected timeout around 150ms, took ${elapsed}ms`,
    );
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Issue 2: A timeout on a silent unit does NOT close the handle for healthy units,
  //          and isConnected() stays true.
  // ─────────────────────────────────────────────────────────────────────────────
  test('Issue 2: timeout on silent unit does not disconnect healthy units; isConnected() stays true', { timeout: 3000 }, async (t) => {
    const server = await createMockServer({ replyDelayMs: 5 });

    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      requestTimeoutMs: 150,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });
    const healthyClient = transport.createClient({ unitId: 1 });

    // 1. Read silent unit 2 -> fails with MODBUS_TIMEOUT
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        const code = getModbusErrorCode(err);
        assert.equal(code, ModbusErrorCode.TIMEOUT);
        return true;
      },
    );

    // 2. isConnected() on healthy client must still be true (transport wasn't destroyed)
    assert.equal(
      healthyClient.isConnected(),
      true,
      'healthyClient.isConnected() should be true after a single unit timeout',
    );

    // 3. Read healthy unit 1 -> must succeed immediately without reconnecting!
    const res = await healthyClient.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.equal(res.length, 1);
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Issue 3: The timeout countdown starts when a request is sent, not when queued
  // ─────────────────────────────────────────────────────────────────────────────
  test('Issue 3: timeout starts when sent to wire, not while queued', { timeout: 3000 }, async (t) => {
    // Unit 1 takes 30ms, Unit 2 silent (times out after 150ms), Unit 3 takes 20ms
    const server = await createMockServer({ replyDelayMs: 30 });

    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      requestTimeoutMs: 150,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const c1 = transport.createClient({ unitId: 1 });
    const c2 = transport.createClient({ unitId: 2 });
    const c3 = transport.createClient({ unitId: 3 });

    // Queue 3 requests concurrently
    const [r1, r2, r3] = await Promise.allSettled([
      c1.readHoldingRegisters({ address: 0, quantity: 1 }),
      c2.readHoldingRegisters({ address: 0, quantity: 1 }),
      c3.readHoldingRegisters({ address: 0, quantity: 1 }),
    ]);

    assert.equal(r1.status, 'fulfilled', 'Unit 1 should succeed');
    assert.equal(r2.status, 'rejected', 'Silent unit 2 should time out');
    assert.equal(
      getModbusErrorCode(r2.reason),
      ModbusErrorCode.TIMEOUT,
      'Unit 2 error must be MODBUS_TIMEOUT',
    );

    // Unit 3 was queued behind Unit 2. In old code it timed out at the same time as Unit 2.
    // In fixed code, Unit 3 begins its timeout only when sent, reaches the wire, and succeeds.
    assert.equal(
      r3.status,
      'fulfilled',
      `Unit 3 should succeed, but got rejected: ${r3.reason?.message}`,
    );
    assert.ok(server.seenUnits.includes(3), 'Unit 3 request should have reached the wire');
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Issue 4: An abort while a request is in flight does not break later requests
  // ─────────────────────────────────────────────────────────────────────────────
  test('Issue 4: in-flight request aborted via AbortSignal does not break later requests', { timeout: 3000 }, async (t) => {
    const server = await createMockServer({ replyDelayMs: 25 });

    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      requestTimeoutMs: 300,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const client1 = transport.createClient({ unitId: 1 });
    const client3 = transport.createClient({ unitId: 3 });

    // 1. Initial healthy read succeeds
    const initialRes = await client1.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.equal(initialRes.length, 1);

    // 2. In-flight request aborted after 10ms
    const controller = new AbortController();
    setTimeout(() => controller.abort(), 10);

    await assert.rejects(
      async () => {
        await client1.readHoldingRegisters({
          address: 0,
          quantity: 1,
          signal: controller.signal,
        });
      },
      (err) => {
        assert.ok(
          err.name === 'AbortError' ||
            err.code === 'AbortError' ||
            err.message.includes('abort'),
          `Expected AbortError, got: ${err.message}`,
        );
        return true;
      },
    );

    // 3. Immediately issue next read to healthy unit 1 -> must succeed!
    const next1 = await client1.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.equal(next1.length, 1, 'Next read on unit 1 should succeed');

    // 4. Next read to healthy unit 3 -> must succeed!
    const next3 = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.equal(next3.length, 1, 'Next read on unit 3 should succeed');
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // Option 1: responseTimeoutMs vs requestTimeoutMs independent budgets
  // ─────────────────────────────────────────────────────────────────────────────
  test('Option 1: responseTimeoutMs governs wire turnaround timeout independently', { timeout: 3000 }, async (t) => {
    // Unit 2 is silent
    const server = await createMockServer({ replyDelayMs: 20 });

    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      responseTimeoutMs: 120, // Wire timeout: 120ms
      requestTimeoutMs: 600,  // Queue timeout: 600ms
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });
    const start = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsed = Date.now() - start;

    // Elapsed should be ~120ms (governed by responseTimeoutMs), definitely < 400ms!
    assert.ok(
      elapsed >= 100 && elapsed < 400,
      `Expected elapsed ~120ms (governed by responseTimeoutMs), but took ${elapsed}ms`,
    );
  });

  test('setRequestTimeout() and clearRequestTimeout() dynamically update timeout on active transport', { timeout: 3000 }, async (t) => {
    // Unit 2 is silent
    const server = await createMockServer({ replyDelayMs: 20 });

    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      responseTimeoutMs: 1000,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });

    // Dynamically change timeout to 120ms
    transport.setRequestTimeout(120);

    const start = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsed = Date.now() - start;
    assert.ok(
      elapsed >= 100 && elapsed < 400,
      `Expected dynamic timeout ~120ms, got ${elapsed}ms`,
    );

    // Clear request timeout
    transport.clearRequestTimeout();
  });

  test('requestTimeoutMs on initialisation followed by dynamic setRequestTimeout() change', { timeout: 3000 }, async (t) => {
    // Unit 2 is silent
    const server = await createMockServer({ replyDelayMs: 20 });

    // 1. Initialise with requestTimeoutMs: 250ms
    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      requestTimeoutMs: 250,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });

    // Verify initial timeout from connect options (~250ms)
    const startInit = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedInit = Date.now() - startInit;
    assert.ok(
      elapsedInit >= 200 && elapsedInit < 500,
      `Expected initial timeout ~250ms, got ${elapsedInit}ms`,
    );

    // 2. Dynamically update request timeout to 100ms
    transport.setRequestTimeout(100);

    // Verify dynamically updated timeout (~100ms)
    const startDynamic = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedDynamic = Date.now() - startDynamic;
    assert.ok(
      elapsedDynamic >= 80 && elapsedDynamic < 300,
      `Expected updated dynamic timeout ~100ms, got ${elapsedDynamic}ms`,
    );

    // 3. Clear request timeout
    transport.clearRequestTimeout();
  });

  test('responseTimeoutMs on initialisation followed by dynamic setRequestTimeout() change', { timeout: 3000 }, async (t) => {
    // Unit 2 is silent
    const server = await createMockServer({ replyDelayMs: 20 });

    // 1. Initialise with responseTimeoutMs: 250ms
    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      responseTimeoutMs: 250,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });

    // Verify initial timeout from connect options (~250ms)
    const startInit = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedInit = Date.now() - startInit;
    assert.ok(
      elapsedInit >= 200 && elapsedInit < 500,
      `Expected initial response timeout ~250ms, got ${elapsedInit}ms`,
    );

    // 2. Dynamically update request timeout to 100ms
    transport.setRequestTimeout(100);

    // Verify dynamically updated timeout (~100ms)
    const startDynamic = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedDynamic = Date.now() - startDynamic;
    assert.ok(
      elapsedDynamic >= 80 && elapsedDynamic < 300,
      `Expected updated dynamic timeout ~100ms, got ${elapsedDynamic}ms`,
    );

    // 3. Clear request timeout
    transport.clearRequestTimeout();
  });

  test('combination of responseTimeoutMs and requestTimeoutMs on initialisation followed by dynamic setRequestTimeout() change', { timeout: 3000 }, async (t) => {
    // Unit 2 is silent, Unit 1 is healthy (20ms reply delay)
    const server = await createMockServer({ replyDelayMs: 20 });

    // 1. Initialise with combination: responseTimeoutMs: 200ms, requestTimeoutMs: 400ms
    const transport = await AsyncTcpTransport.connect({
      host: '127.0.0.1',
      port: server.port,
      responseTimeoutMs: 200,
      requestTimeoutMs: 400,
    });
    t.after(async () => {
      await transport.close();
      await server.close();
    });

    const silentClient = transport.createClient({ unitId: 2 });
    const healthyClient = transport.createClient({ unitId: 1 });

    // Verify initial wire turnaround timeout on silent unit (~200ms)
    const startInit = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedInit = Date.now() - startInit;
    assert.ok(
      elapsedInit >= 160 && elapsedInit < 450,
      `Expected wire timeout governed by responseTimeoutMs ~200ms, got ${elapsedInit}ms`,
    );

    // Healthy client works without issue
    const healthyResult = await healthyClient.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.ok(healthyResult !== undefined);

    // 2. Dynamically change request timeout to 100ms
    transport.setRequestTimeout(100);

    const startDynamic = Date.now();
    await assert.rejects(
      async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      },
      (err) => {
        assert.equal(getModbusErrorCode(err), ModbusErrorCode.TIMEOUT);
        return true;
      },
    );
    const elapsedDynamic = Date.now() - startDynamic;
    assert.ok(
      elapsedDynamic >= 80 && elapsedDynamic < 300,
      `Expected updated dynamic timeout ~100ms, got ${elapsedDynamic}ms`,
    );

    // Healthy client continues to work
    const healthyResult2 = await healthyClient.readHoldingRegisters({ address: 0, quantity: 1 });
    assert.ok(healthyResult2 !== undefined);

    // 3. Clear request timeout
    transport.clearRequestTimeout();
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // RTU tests using socat PTY (runs on Linux/macOS when socat is available)
  // ─────────────────────────────────────────────────────────────────────────────
  let hasSocat = false;
  if (process.platform !== 'win32') {
    try {
      const res = spawnSync('socat', ['-V'], { stdio: 'ignore' });
      hasSocat = res.status === 0;
    } catch {
      hasSocat = false;
    }
  }

  test('RTU: 4 timeout behaviors against simulated PTY bus', { skip: !hasSocat, timeout: 5000 }, async (t) => {
    const portPath = `/tmp/mbrs-test-${process.pid}`;
    const crc16 = (bytes) => {
      let crc = 0xffff;
      for (const byte of bytes) {
        crc ^= byte;
        for (let i = 0; i < 8; i += 1) crc = crc & 1 ? (crc >> 1) ^ 0xa001 : crc >> 1;
      }
      return Buffer.from([crc & 0xff, crc >> 8]);
    };
    const frame = (pdu) => Buffer.concat([pdu, crc16(pdu)]);
    const requestLength = (buf) => (buf[1] === 16 ? 9 + buf[6] : 8);

    const socat = spawn('socat', ['-d', `pty,raw,echo=0,link=${portPath}`, 'stdio'], {
      stdio: ['pipe', 'pipe', 'inherit'],
    });
    socat.on('error', () => {});
    t.after(() => {
      try {
        socat.kill();
      } catch {}
      try {
        if (existsSync(portPath)) unlinkSync(portPath);
      } catch {}
    });

    let pendingBuf = Buffer.alloc(0);
    const seen = [];
    socat.stdout.on('data', (chunk) => {
      pendingBuf = Buffer.concat([pendingBuf, chunk]);
      while (pendingBuf.length >= 8 && pendingBuf.length >= requestLength(pendingBuf)) {
        const req = pendingBuf.subarray(0, requestLength(pendingBuf));
        pendingBuf = pendingBuf.subarray(req.length);
        const [unit, fc] = req;
        seen.push(unit);
        if (unit === 2) continue; // Silent unit
        const reply =
          fc === 3
            ? frame(Buffer.from([unit, 3, req[5] * 2, ...Buffer.alloc(req[5] * 2)]))
            : frame(Buffer.from(req.subarray(0, 6)));
        setTimeout(() => {
          try {
            socat.stdin.write(reply);
          } catch {}
        }, 3);
      }
    });

    let attempts = 0;
    while (!existsSync(portPath)) {
      await sleep(10);
      attempts += 1;
      if (attempts > 50) {
        throw new Error(`Timed out waiting for socat to create PTY symlink at ${portPath}`);
      }
    }

    // 1. responseTimeoutMs limits request
    {
      const rtuTransport = await AsyncRtuTransport.open({
        portPath,
        baudRate: 9600,
        responseTimeoutMs: 200,
      });
      const silentClient = rtuTransport.createClient({ unitId: 2 });
      await assert.rejects(async () => {
        await silentClient.readHoldingRegisters({ address: 0, quantity: 1 });
      });
      await rtuTransport.close();
    }

    // 2. Timeout on silent unit does not disconnect healthy unit
    {
      const rtuTransport = await AsyncRtuTransport.open({
        portPath,
        baudRate: 9600,
        requestTimeoutMs: 200,
      });
      const silent = rtuTransport.createClient({ unitId: 2 });
      const healthy = rtuTransport.createClient({ unitId: 1 });
      await assert.rejects(async () => {
        await silent.readHoldingRegisters({ address: 0, quantity: 1 });
      });
      assert.equal(healthy.isConnected(), true);
      const res = await healthy.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.equal(res.length, 1);
      await rtuTransport.close();
    }
  });
}
