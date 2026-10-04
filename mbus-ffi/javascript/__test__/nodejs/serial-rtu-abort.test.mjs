import { test, describe, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
import { setTimeout as sleep } from 'node:timers/promises';

let modbus;
try {
  modbus = await import('../../dist/index.js');
} catch {
  test('serial rtu abort suite (skipped - native addon not built)', { skip: true }, () => {});
}

const hasSocat = process.platform !== 'win32' && spawnSync('which', ['socat']).status === 0;

if (modbus && hasSocat) {
  const { AsyncRtuTransport } = modbus;

  // CRC-16 calculation for Modbus RTU frames
  function calcCrc(buf) {
    let crc = 0xffff;
    for (const b of buf) {
      crc ^= b;
      for (let i = 0; i < 8; i++) {
        crc = crc & 1 ? (crc >> 1) ^ 0xa001 : crc >> 1;
      }
    }
    return Buffer.from([crc & 0xff, (crc >> 8) & 0xff]);
  }

  function frame(pdu) {
    return Buffer.concat([pdu, calcCrc(pdu)]);
  }

  describe('Modbus RTU Abort & Desync Suite (socat PTY)', () => {
    let portPath;
    let socat;
    let transport;
    const timers = new Set();
    let requestCount = 0;

    before(async () => {
      portPath = `/tmp/modbus-rtu-test-${process.pid}-${Date.now()}`;
      socat = spawn('socat', [`pty,raw,echo=0,link=${portPath}`, 'stdio']);

      let buffered = Buffer.alloc(0);
      socat.stdout.on('data', (chunk) => {
        buffered = Buffer.concat([buffered, chunk]);
        while (buffered.length >= 8) {
          const req = buffered.subarray(0, 8);
          buffered = buffered.subarray(8);
          requestCount++;

          const unit = req[0];
          const fc = req[1];

          // Unit 2 is completely silent (never replies)
          if (unit === 2) {
            continue;
          }

          let respPayload;
          if (fc === 3 || fc === 4) {
            // Echo unit ID as value: [unit, fc, byteCount, val_hi, val_lo]
            respPayload = Buffer.from([unit, fc, 2, 0, unit]);
          } else if (fc === 6) {
            // FC06 echoes request bytes 0..6
            respPayload = req.subarray(0, 6);
          } else {
            continue;
          }

          const reply = frame(respPayload);
          // Simulate turnaround latency: Unit 1 has 100ms, Unit 3 has 5ms
          const delayMs = unit === 1 ? 100 : 10;
          const timer = setTimeout(() => {
            timers.delete(timer);
            socat.stdin.write(reply);
          }, delayMs);
          timers.add(timer);
        }
      });

      while (!existsSync(portPath)) {
        await sleep(5);
      }

      transport = await AsyncRtuTransport.open({
        portPath,
        baudRate: 115200,
        responseTimeoutMs: 300,
      });
      transport.setRequestTimeout(300);
    });

    after(async () => {
      if (transport) {
        await transport.close().catch(() => {});
      }
      for (const t of timers) clearTimeout(t);
      if (socat) {
        socat.kill('SIGKILL');
      }
      if (portPath) {
        rmSync(portPath, { force: true });
      }
    });

    test('Scenario 1: in-flight abort of slow unit 1 leaves unit 3 intact', async () => {
      const client1 = transport.createClient({ unitId: 1 });
      const client3 = transport.createClient({ unitId: 3 });

      // Warmup read to Unit 1
      const warmup = await client1.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(warmup[0], 1);

      // In-flight abort of Unit 1
      const controller = new AbortController();
      const beforeReq = requestCount;
      const pendingAbort = client1.readHoldingRegisters({
        address: 0,
        quantity: 1,
        signal: controller.signal,
      });

      while (requestCount === beforeReq) {
        await sleep(1);
      }
      await sleep(20);
      controller.abort();

      await assert.rejects(pendingAbort, (err) => {
        return err.message.includes('aborted');
      });

      // Immediate subsequent read to healthy Unit 3 must succeed without ChecksumError
      const nextRead = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(nextRead[0], 3);
    });

    test('Scenario 2: in-flight abort of silent unit 2 unblocks on response timeout', async () => {
      const client2 = transport.createClient({ unitId: 2 });
      const client3 = transport.createClient({ unitId: 3 });

      const controller = new AbortController();
      const beforeReq = requestCount;
      const pendingAbort = client2.readHoldingRegisters({
        address: 0,
        quantity: 1,
        signal: controller.signal,
      });

      while (requestCount === beforeReq) {
        await sleep(1);
      }
      await sleep(20);
      controller.abort();

      await assert.rejects(pendingAbort, (err) => {
        return err.message.includes('aborted');
      });

      // Unit 3 will queue, wait for Unit 2's turnaround window to elapse, and succeed
      const nextRead = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(nextRead[0], 3);
    });

    test('Scenario 3: abort while queued purges request without touching the wire', async () => {
      const client1 = transport.createClient({ unitId: 1 });
      const client3 = transport.createClient({ unitId: 3 });

      // In-flight slow read to Unit 1
      const p1 = client1.readHoldingRegisters({ address: 0, quantity: 1 });

      // Enqueue Unit 1 (aborted) and Unit 3 behind it
      const controller = new AbortController();
      const p2 = client1.readHoldingRegisters({
        address: 0,
        quantity: 1,
        signal: controller.signal,
      });
      const p3 = client3.readHoldingRegisters({ address: 0, quantity: 1 });

      // Abort p2 while it's still waiting in queue
      controller.abort();

      const [r1, r2, r3] = await Promise.allSettled([p1, p2, p3]);

      assert.strictEqual(r1.status, 'fulfilled');
      assert.strictEqual(r2.status, 'rejected');
      assert.ok(r2.reason.message.includes('aborted'));
      assert.strictEqual(r3.status, 'fulfilled');
      assert.strictEqual(r3.value[0], 3);
    });

    test('Scenario 4: already-aborted signal rejects immediately', async () => {
      const client3 = transport.createClient({ unitId: 3 });
      const signal = AbortSignal.abort();

      await assert.rejects(
        async () => client3.readHoldingRegisters({ address: 0, quantity: 1, signal }),
        (err) => err.message.includes('aborted'),
      );

      const nextRead = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(nextRead[0], 3);
    });

    test('Scenario 5: in-flight abort on write operations (FC06)', async () => {
      const client1 = transport.createClient({ unitId: 1 });
      const client3 = transport.createClient({ unitId: 3 });

      const controller = new AbortController();
      const beforeReq = requestCount;
      const pendingWrite = client1.writeSingleRegister({
        address: 0,
        value: 42,
        signal: controller.signal,
      });

      while (requestCount === beforeReq) {
        await sleep(1);
      }
      await sleep(20);
      controller.abort();

      await assert.rejects(pendingWrite, (err) => err.message.includes('aborted'));

      // Subsequent read to Unit 3 must succeed without CRC errors
      const nextRead = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(nextRead[0], 3);
    });

    test('Scenario 6: consecutive rapid in-flight aborts maintain bus sanity', async () => {
      const client1 = transport.createClient({ unitId: 1 });
      const client3 = transport.createClient({ unitId: 3 });

      // First in-flight abort
      const c1 = new AbortController();
      const p1 = client1.readHoldingRegisters({ address: 0, quantity: 1, signal: c1.signal });
      await sleep(15);
      c1.abort();
      await assert.rejects(p1, (err) => err.message.includes('aborted'));

      // Second in-flight abort
      const c2 = new AbortController();
      const p2 = client1.readHoldingRegisters({ address: 0, quantity: 1, signal: c2.signal });
      await sleep(15);
      c2.abort();
      await assert.rejects(p2, (err) => err.message.includes('aborted'));

      // Third request must succeed cleanly
      const p3 = await client3.readHoldingRegisters({ address: 0, quantity: 1 });
      assert.strictEqual(p3[0], 3);
    });
  });
} else if (!hasSocat) {
  test('serial rtu abort suite (skipped - socat not available on this platform)', { skip: true }, () => {});
}
