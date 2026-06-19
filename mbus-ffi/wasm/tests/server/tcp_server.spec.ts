import { describe, it, expect } from 'vitest';
import init, { WasmTcpServer } from '../../dist/web/modbus-rs.js';

await init(); // initialise WASM in browser context

const WS_PORT = import.meta.env.VITE_WS_TEST_PORT; // injected by globalSetup

describe('WasmTcpServer — browser E2E', () => {

  it('exception flow: FC03 returns exception 0x83/0x02', async () => {
    const url = `ws://127.0.0.1:${WS_PORT}/server-exc`;
    const server = await WasmTcpServer.bind(
      { wsUrl: url, unitId: 1 },
      { onReadHoldingRegisters: () => ({ exception: 2 }) }
    );
    server.serve().catch(() => {});

    // Connect a plain browser WebSocket client
    const client = new WebSocket(url);
    await new Promise<void>(r => { client.onopen = () => r(); });
    client.binaryType = 'arraybuffer';

    // FC03 request frame
    const req = new Uint8Array([0x00,0x01,0x00,0x00,0x00,0x06,0x01,0x03,0x00,0x64,0x00,0x03]);
    client.send(req);

    const response = await new Promise<Uint8Array>(resolve => {
      client.onmessage = e => resolve(new Uint8Array(e.data as ArrayBuffer));
    });

    expect(response[7]).toBe(0x83); // exception FC
    expect(response[8]).toBe(0x02); // IllegalDataAddress

    client.close();
    await server.shutdown();
  });

  it('happy path: FC03 handler returns registers', async () => {
    const url = `ws://127.0.0.1:${WS_PORT}/server-happy`;
    const server = await WasmTcpServer.bind(
      { wsUrl: url, unitId: 1 },
      { onReadHoldingRegisters: (req: any) => [req.unitId, req.address, req.quantity] }
    );
    server.serve().catch(() => {});

    const client = new WebSocket(url);
    await new Promise<void>(r => { client.onopen = () => r(); });
    client.binaryType = 'arraybuffer';

    const req = new Uint8Array([0x00,0x01,0x00,0x00,0x00,0x06,0x01,0x03,0x00,0x64,0x00,0x03]);
    client.send(req);

    const response = await new Promise<Uint8Array>(resolve => {
      client.onmessage = e => resolve(new Uint8Array(e.data as ArrayBuffer));
    });

    expect(response[7]).toBe(0x03); // FC03 response
    expect(response[8]).toBe(0x06); // byte_count = 6 (3 registers × 2)

    client.close();
    await server.shutdown();
  });

  it('happy path: FC43 custom handler returns device identification', async () => {
    const url = `ws://127.0.0.1:${WS_PORT}/server-device-id-happy`;
    const server = await WasmTcpServer.bind(
      { wsUrl: url, unitId: 1 },
      {
        onReadDeviceIdentification: (req: any) => {
          expect(req.unitId).toBe(1);
          expect(req.readDeviceIdCode).toBe(1);
          expect(req.objectId).toBe(0);
          return {
            conformityLevel: 1,
            moreFollows: false,
            nextObjectId: 0,
            objects: [
              { id: 0, value: 'CustomVendor' }
            ]
          };
        }
      }
    );
    server.serve().catch(() => {});

    const client = new WebSocket(url);
    await new Promise<void>(r => { client.onopen = () => r(); });
    client.binaryType = 'arraybuffer';

    const req = new Uint8Array([0x00,0x01,0x00,0x00,0x00,0x05,0x01,0x2B,0x0E,0x01,0x00]);
    client.send(req);

    const response = await new Promise<Uint8Array>(resolve => {
      client.onmessage = e => resolve(new Uint8Array(e.data as ArrayBuffer));
    });

    expect(response[7]).toBe(0x2B); // FC 43 (0x2B)
    expect(response[8]).toBe(0x0E); // MEI Type
    expect(response[9]).toBe(0x01); // Read Device ID Code
    expect(response[10]).toBe(0x01); // Conformity level
    expect(response[11]).toBe(0x00); // More follows
    expect(response[12]).toBe(0x00); // Next object ID
    expect(response[13]).toBe(0x01); // Number of objects (custom vendor)
    expect(response[14]).toBe(0x00); // Object ID (0)
    expect(response[15]).toBe(0x0C); // Object Length (12)
    
    const valueBytes = response.slice(16, 28);
    const valueStr = new TextDecoder().decode(valueBytes);
    expect(valueStr).toBe('CustomVendor');

    client.close();
    await server.shutdown();
  });

  it('fallback: FC43 handler fallback returns default device identification', async () => {
    const url = `ws://127.0.0.1:${WS_PORT}/server-device-id-fallback`;
    const server = await WasmTcpServer.bind(
      { wsUrl: url, unitId: 1 },
      {} // no onReadDeviceIdentification callback
    );
    server.serve().catch(() => {});

    const client = new WebSocket(url);
    await new Promise<void>(r => { client.onopen = () => r(); });
    client.binaryType = 'arraybuffer';

    const req = new Uint8Array([0x00,0x01,0x00,0x00,0x00,0x05,0x01,0x2B,0x0E,0x01,0x00]);
    client.send(req);

    const response = await new Promise<Uint8Array>(resolve => {
      client.onmessage = e => resolve(new Uint8Array(e.data as ArrayBuffer));
    });

    expect(response[7]).toBe(0x2B); // FC 43 (0x2B)
    expect(response[8]).toBe(0x0E); // MEI Type
    expect(response[9]).toBe(0x01); // Read Device ID Code
    expect(response[10]).toBe(0x82); // Default conformity level (0x82)

    client.close();
    await server.shutdown();
  });
});
