import { describe, it, expect } from 'vitest';
import init, { WasmTcpTransport, WasmModbusClient } from '../../dist/web/modbus-rs.js';

await init(); // initialize WASM in browser context

const WS_PORT = import.meta.env.VITE_WS_TEST_PORT; // injected by globalSetup

interface MockServerContext {
  client: WasmModbusClient;
  transport: WasmTcpTransport;
  mockServer: WebSocket;
  cleanup: () => Promise<void>;
}

let nextTestId = 0;
async function setupClientAndMockServer(unitId: number = 1, timeoutMs: number = 2000): Promise<MockServerContext> {
  const url = `ws://127.0.0.1:${WS_PORT}/client-test-${nextTestId++}`;

  // 1. Setup mock server ws connection
  const mockServer = new WebSocket(url);
  await new Promise<void>(resolve => { mockServer.onopen = () => resolve(); });
  mockServer.binaryType = 'arraybuffer';

  // 2. Setup Wasm client
  const transport = await WasmTcpTransport.connect(url, { responseTimeoutMs: timeoutMs });
  const client = transport.createClient({ unitId });

  const cleanup = async () => {
    mockServer.close();
    transport.close();
    // Give a brief moment for connection closure to propagate
    await new Promise<void>(resolve => setTimeout(resolve, 10));
  };

  return { client, transport, mockServer, cleanup };
}

// Helper to handle one request and reply with custom PDU data
function handleRequestOnce(mockServer: WebSocket, expectedFc: number, buildPduResponse: (reqPdu: Uint8Array) => number[]) {
  return new Promise<void>((resolve, reject) => {
    mockServer.onmessage = (e) => {
      try {
        const req = new Uint8Array(e.data as ArrayBuffer);
        // req: [txn_hi, txn_lo, proto_hi, proto_lo, len_hi, len_lo, unit_id, fc, ...]
        expect(req[7]).toBe(expectedFc);

        const txnId = (req[0] << 8) | req[1];
        const unitId = req[6];
        const reqPdu = req.slice(7);

        const respPdu = buildPduResponse(reqPdu);
        const len = respPdu.length + 1; // +1 for unitId

        const resp = new Uint8Array([
          req[0], req[1], // txn_id
          0x00, 0x00,     // proto_id
          (len >> 8) & 0xff, len & 0xff, // length
          unitId,
          ...respPdu
        ]);

        mockServer.send(resp);
        resolve();
      } catch (err) {
        reject(err);
      }
    };
  });
}

describe('WasmModbusClient — browser E2E', () => {

  it('FC03 — readHoldingRegisters', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x03, (reqPdu) => {
      // reqPdu: [FC, addr_hi, addr_lo, qty_hi, qty_lo]
      expect(reqPdu[1]).toBe(0x00);
      expect(reqPdu[2]).toBe(0x6B); // address = 107 (0x006B)
      expect(reqPdu[4]).toBe(0x02); // quantity = 2
      // Reply: FC, byte_count, values (0x1234, 0x5678)
      return [0x03, 0x04, 0x12, 0x34, 0x56, 0x78];
    });

    const regs = await client.readHoldingRegisters({ address: 0x006B, quantity: 2 });
    await handlerPromise;

    expect(regs).toBeInstanceOf(Uint16Array);
    expect(regs.length).toBe(2);
    expect(regs[0]).toBe(0x1234);
    expect(regs[1]).toBe(0x5678);

    await cleanup();
  });

  it('FC04 — readInputRegisters', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x04, (reqPdu) => {
      expect(reqPdu[2]).toBe(0x05); // address = 5
      expect(reqPdu[4]).toBe(0x01); // quantity = 1
      return [0x04, 0x02, 0xAA, 0x55];
    });

    const regs = await client.readInputRegisters({ address: 5, quantity: 1 });
    await handlerPromise;

    expect(regs.length).toBe(1);
    expect(regs[0]).toBe(0xAA55);

    await cleanup();
  });

  it('FC06 — writeSingleRegister', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x06, (reqPdu) => {
      // reqPdu: [FC, addr_hi, addr_lo, val_hi, val_lo]
      expect(reqPdu[2]).toBe(0x0A); // address = 10
      expect(reqPdu[3]).toBe(0x00);
      expect(reqPdu[4]).toBe(0xFF); // value = 255
      // Echo back the request PDU
      return Array.from(reqPdu);
    });

    await client.writeSingleRegister({ address: 10, value: 0x00FF });
    await handlerPromise;

    await cleanup();
  });

  it('FC16 — writeMultipleRegisters', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x10, (reqPdu) => {
      // reqPdu: [FC, addr_hi, addr_lo, qty_hi, qty_lo, byte_count, val1_hi, val1_lo, ...]
      expect(reqPdu[2]).toBe(0x14); // address = 20
      expect(reqPdu[4]).toBe(0x02); // quantity = 2
      expect(reqPdu[5]).toBe(0x04); // byte count = 4
      expect((reqPdu[6] << 8) | reqPdu[7]).toBe(0x1111);
      expect((reqPdu[8] << 8) | reqPdu[9]).toBe(0x2222);
      // Reply: FC, addr_hi, addr_lo, qty_hi, qty_lo
      return [0x10, 0x00, 0x14, 0x00, 0x02];
    });

    await client.writeMultipleRegisters({ address: 20, values: [0x1111, 0x2222] });
    await handlerPromise;

    await cleanup();
  });

  it('FC23 — readWriteMultipleRegisters', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x17, (reqPdu) => {
      // reqPdu: [FC, read_addr_hi, read_addr_lo, read_qty_hi, read_qty_lo, write_addr_hi, write_addr_lo, write_qty_hi, write_qty_lo, write_byte_count, write_val1_hi, write_val1_lo...]
      expect(reqPdu[2]).toBe(0x01); // read address = 1
      expect(reqPdu[4]).toBe(0x02); // read quantity = 2
      expect(reqPdu[6]).toBe(0x03); // write address = 3
      expect(reqPdu[8]).toBe(0x01); // write quantity = 1
      // Reply read registers values (e.g. 0x3333, 0x4444)
      return [0x17, 0x04, 0x33, 0x33, 0x44, 0x44];
    });

    const regs = await client.readWriteMultipleRegisters({
      readAddress: 1,
      readQuantity: 2,
      writeAddress: 3,
      writeValues: [0x5555]
    });
    await handlerPromise;

    expect(regs.length).toBe(2);
    expect(regs[0]).toBe(0x3333);
    expect(regs[1]).toBe(0x4444);

    await cleanup();
  });

  it('FC01 — readCoils', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x01, (reqPdu) => {
      expect(reqPdu[2]).toBe(0x64); // address = 100
      expect(reqPdu[4]).toBe(0x05); // quantity = 5
      // Reply: FC, byte_count, coil_bytes (1, 1, 0, 1, 0 -> 0x0D)
      return [0x01, 0x01, 0x0D];
    });

    const coils = await client.readCoils({ address: 100, quantity: 5 });
    await handlerPromise;

    expect(coils).toEqual([true, false, true, true, false]);

    await cleanup();
  });

  it('FC05 — writeSingleCoil', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x05, (reqPdu) => {
      expect(reqPdu[2]).toBe(0x0A); // address = 10
      expect(reqPdu[3]).toBe(0xFF); // true (0xFF00)
      expect(reqPdu[4]).toBe(0x00);
      return Array.from(reqPdu);
    });

    await client.writeSingleCoil({ address: 10, value: true });
    await handlerPromise;

    await cleanup();
  });

  it('FC15 — writeMultipleCoils', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x0F, (reqPdu) => {
      // reqPdu: [FC, addr_hi, addr_lo, qty_hi, qty_lo, byte_count, value_bytes]
      expect(reqPdu[2]).toBe(0x14); // address = 20
      expect(reqPdu[4]).toBe(0x03); // quantity = 3
      expect(reqPdu[5]).toBe(0x01); // byte count = 1
      expect(reqPdu[6]).toBe(0x05); // values: true, false, true -> binary 101 -> 0x05
      return [0x0F, 0x00, 0x14, 0x00, 0x03];
    });

    await client.writeMultipleCoils({ address: 20, values: [true, false, true] });
    await handlerPromise;

    await cleanup();
  });

  it('FC02 — readDiscreteInputs', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x02, (reqPdu) => {
      expect(reqPdu[2]).toBe(0x64); // address = 100
      expect(reqPdu[4]).toBe(0x03); // quantity = 3
      return [0x02, 0x01, 0x05]; // true, false, true -> 0x05
    });

    const inputs = await client.readDiscreteInputs({ address: 100, quantity: 3 });
    await handlerPromise;

    expect(inputs).toEqual([true, false, true]);

    await cleanup();
  });

  it('FC22 — maskWriteRegister', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x16, (reqPdu) => {
      // reqPdu: [FC, addr_hi, addr_lo, and_hi, and_lo, or_hi, or_lo]
      expect(reqPdu[2]).toBe(0x05); // address = 5
      expect((reqPdu[3] << 8) | reqPdu[4]).toBe(0x00FF); // AND mask
      expect((reqPdu[5] << 8) | reqPdu[6]).toBe(0xFF00); // OR mask
      return Array.from(reqPdu);
    });

    await client.maskWriteRegister({ address: 5, andMask: 0x00FF, orMask: 0xFF00 });
    await handlerPromise;

    await cleanup();
  });

  it('FC18 — readFifoQueue', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x18, (reqPdu) => {
      expect(reqPdu[2]).toBe(0x0A); // address = 10
      // Reply: FC, byte_count_hi, byte_count_lo, fifo_count_hi, fifo_count_lo, val1_hi, val1_lo, val2_hi, val2_lo
      // byte_count = 6, fifo_count = 2, values = [0x1234, 0x5678]
      return [0x18, 0x00, 0x06, 0x00, 0x02, 0x12, 0x34, 0x56, 0x78];
    });

    const fifo = await client.readFifoQueue({ address: 10 });
    await handlerPromise;

    expect(fifo.length).toBe(2);
    expect(fifo[0]).toBe(0x1234);
    expect(fifo[1]).toBe(0x5678);

    await cleanup();
  });

  it('FC14 — readFileRecord', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x14, (reqPdu) => {
      // reqPdu: [FC, byte_count, sub_req_type, file_num_hi, file_num_lo, rec_num_hi, rec_num_lo, len_hi, len_lo]
      expect(reqPdu[1]).toBe(0x07); // sub request byte count
      expect(reqPdu[2]).toBe(0x06); // type reference
      expect(reqPdu[4]).toBe(0x01); // file = 1
      expect(reqPdu[6]).toBe(0x02); // record = 2
      expect(reqPdu[8]).toBe(0x02); // length = 2
      // Reply: FC, resp_byte_count, sub_resp_len, sub_resp_type, val1_hi, val1_lo, val2_hi, val2_lo
      return [0x14, 0x06, 0x05, 0x06, 0xAA, 0x55, 0x55, 0xAA];
    });

    const records = await client.readFileRecord({
      requests: [{ fileNumber: 1, recordNumber: 2, recordLength: 2 }]
    });
    await handlerPromise;

    expect(records.length).toBe(1);
    expect(records[0]).toBeInstanceOf(Uint16Array);
    expect(records[0].length).toBe(2);
    expect(records[0][0]).toBe(0xAA55);
    expect(records[0][1]).toBe(0x55AA);

    await cleanup();
  });

  it('FC15 — writeFileRecord', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x15, (reqPdu) => {
      // reqPdu: [FC, byte_count, sub_req_type, file_num_hi, file_num_lo, rec_num_hi, rec_num_lo, len_hi, len_lo, val1_hi, val1_lo, ...]
      expect(reqPdu[1]).toBe(0x09); // byte count = 9
      expect(reqPdu[2]).toBe(0x06); // type = 6
      expect(reqPdu[4]).toBe(0x01); // file = 1
      expect(reqPdu[6]).toBe(0x02); // record = 2
      expect(reqPdu[8]).toBe(0x01); // length = 1
      expect((reqPdu[9] << 8) | reqPdu[10]).toBe(0xDEAD);
      // Echo response
      return Array.from(reqPdu);
    });

    await client.writeFileRecord({
      requests: [{ fileNumber: 1, recordNumber: 2, recordData: [0xDEAD] }]
    });
    await handlerPromise;

    await cleanup();
  });

  it('FC07 — readExceptionStatus', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x07, (reqPdu) => {
      // Reply: FC, status byte (0x55)
      return [0x07, 0x55];
    });

    const status = await client.readExceptionStatus();
    await handlerPromise;

    expect(status).toBe(0x55);

    await cleanup();
  });

  it('FC08 — diagnostics', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x08, (reqPdu) => {
      // reqPdu: [FC, sub_hi, sub_lo, data_hi, data_lo, ...]
      expect(reqPdu[2]).toBe(0x00); // subFunction = 0 (Return Query Data)
      expect(reqPdu[4]).toBe(0x34); // data value
      // Return query data echo
      return Array.from(reqPdu);
    });

    const resp = await client.diagnostics({ subFunction: 0, data: [0x1234] });
    await handlerPromise;

    expect(resp.subFunction).toBe(0);
    expect(resp.data).toBeInstanceOf(Uint16Array);
    expect(resp.data[0]).toBe(0x1234);

    await cleanup();
  });

  it('FC43/14 — readDeviceIdentification', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer();

    const handlerPromise = handleRequestOnce(mockServer, 0x2B, (reqPdu) => {
      // reqPdu: [FC, 0x0E (MEI type), readDeviceIdCode, objectId]
      expect(reqPdu[1]).toBe(0x0E);
      expect(reqPdu[2]).toBe(0x01); // readDeviceIdCode
      expect(reqPdu[3]).toBe(0x00); // objectId
      // Reply: FC, MEI (0x0E), readDeviceIdCode, conformityLevel (0x01), moreFollows (0x00), nextObjectId (0x00), number_of_objects (1), obj_id (0), obj_len (4), obj_data ('Test')
      return [
        0x2B, 0x0E, 0x01, 0x01, 0x00, 0x00, 0x01,
        0x00, 0x04, 0x54, 0x65, 0x73, 0x74
      ];
    });

    const resp = await client.readDeviceIdentification({ readDeviceIdCode: 1, objectId: 0 });
    await handlerPromise;

    expect(resp.conformityLevel).toBe(1);
    expect(resp.moreFollows).toBe(false);
    expect(resp.objects.length).toBe(1);
    expect(resp.objects[0].id).toBe(0);
    expect(resp.objects[0].value).toBe('Test');

    await cleanup();
  });

  it('timeout: request rejects with Timeout error', async () => {
    const { client, mockServer, cleanup } = await setupClientAndMockServer(1, 50); // 50ms timeout

    // We do NOT call mockServer.send or handleRequestOnce to simulate a non-responding server
    expect(client.pendingRequests).toBe(false);
    const promise = client.readHoldingRegisters({ address: 10, quantity: 1 });
    expect(client.pendingRequests).toBe(true);

    await expect(promise).rejects.toThrow('Timeout');
    expect(client.pendingRequests).toBe(false);

    await cleanup();
  });

  it('reconnect: transport reconnects successfully', async () => {
    const { client, transport, mockServer, cleanup } = await setupClientAndMockServer();

    // Verify it is connected initially
    expect(client.isConnected()).toBe(true);

    // Call reconnect
    await transport.reconnect();

    // Verify it is still connected/active
    expect(client.isConnected()).toBe(true);

    await cleanup();
  });
});
