# modbus-rs

High-performance Modbus TCP/RTU/ASCII client, server, and gateway for Node.js, powered by Rust.

## Features

- **Async/Promise-based API** - All operations return Promises
- **TCP Client** - Full Modbus TCP/IP client implementation
- **Serial Client** - Modbus RTU and ASCII over serial port
- **TCP Server** - Build Modbus TCP servers with JavaScript handlers
- **TCP Gateway** - Route requests to multiple downstream servers based on unit ID
- **High Performance** - Native Rust core with napi-rs bindings
- **Type Safe** - Full TypeScript definitions included
- **Cross Platform** - Pre-built binaries for Linux, macOS, and Windows

## Installation

```bash
npm install modbus-rs
```

## Quick Start

### TCP Client

```javascript
const { AsyncTcpModbusClient } = require('modbus-rs');

async function main() {
  const client = await AsyncTcpModbusClient.connect({
    host: '127.0.0.1',
    port: 502,
    unitId: 1,
    timeoutMs: 5000,
  });

  try {
    // Read holding registers (FC03)
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Registers:', registers);

    // Write single register (FC06)
    await client.writeSingleRegister({
      address: 0,
      value: 12345,
    });
  } finally {
    await client.close();
  }
}

main().catch(console.error);
```

### Serial RTU Client

```javascript
const { AsyncSerialModbusClient } = require('modbus-rs');

async function main() {
  const client = await AsyncSerialModbusClient.connectRtu({
    portPath: '/dev/ttyUSB0',
    unitId: 1,
    baudRate: 19200,
    dataBits: 8,
    stopBits: 1,
    parity: 'even',
  });

  try {
    const registers = await client.readHoldingRegisters({
      address: 0,
      quantity: 10,
    });
    console.log('Registers:', registers);
  } finally {
    await client.close();
  }
}

main().catch(console.error);
```

### TCP Server

```javascript
const { AsyncTcpModbusServer } = require('modbus-rs');

const holdingRegisters = new Array(1000).fill(0);

async function main() {
  const server = await AsyncTcpModbusServer.bind(
    { host: '0.0.0.0', port: 502 },
    {
      onReadHoldingRegisters: (req) => {
        return holdingRegisters.slice(req.address, req.address + req.count);
      },
      onWriteSingleRegister: (req) => {
        holdingRegisters[req.address] = req.value;
        return true;
      },
    }
  );

  console.log('Server listening on port 502');
  
  process.on('SIGINT', async () => {
    await server.shutdown();
    process.exit(0);
  });
}

main().catch(console.error);
```

### TCP Gateway

```javascript
const { AsyncTcpGateway } = require('modbus-rs');

async function main() {
  const gateway = await AsyncTcpGateway.bind(
    { host: '0.0.0.0', port: 502 },
    {
      downstreams: [
        { host: '192.168.1.10', port: 502 },
        { host: '192.168.1.11', port: 502 },
      ],
      routes: [
        { unitId: 1, channel: 0 },
        { unitId: 2, channel: 1 },
      ],
    }
  );

  console.log('Gateway listening on port 502');
  
  process.on('SIGINT', async () => {
    await gateway.shutdown();
    process.exit(0);
  });
}

main().catch(console.error);
```

## API Reference

### AsyncTcpModbusClient

- `connect(opts: TcpClientOptions)` - Connect to a Modbus TCP server
- `close()` - Close the connection
- `readCoils(opts)` - FC01: Read Coils
- `readDiscreteInputs(opts)` - FC02: Read Discrete Inputs
- `readHoldingRegisters(opts)` - FC03: Read Holding Registers
- `readInputRegisters(opts)` - FC04: Read Input Registers
- `writeSingleCoil(opts)` - FC05: Write Single Coil
- `writeSingleRegister(opts)` - FC06: Write Single Register
- `writeMultipleCoils(opts)` - FC15: Write Multiple Coils
- `writeMultipleRegisters(opts)` - FC16: Write Multiple Registers
- `readWriteMultipleRegisters(opts)` - FC23: Read/Write Multiple Registers
- `readFileRecord(opts)` - FC20: Read File Record
- `writeFileRecord(opts)` - FC21: Write File Record
- `readFifoQueue(opts)` - FC24: Read FIFO Queue
- `readExceptionStatus()` - FC07: Read Exception Status
- `diagnostics(opts)` - FC08: Diagnostics
- `readDeviceIdentification(opts)` - FC43/14: Read Device Identification

### AsyncSerialModbusClient

Same methods as `AsyncTcpModbusClient`, with different connection options:

- `connectRtu(opts: SerialClientOptions)` - Connect using Modbus RTU
- `connectAscii(opts: SerialClientOptions)` - Connect using Modbus ASCII

### AsyncTcpModbusServer

- `bind(opts, handlers)` - Create and start a TCP server
- `shutdown()` - Stop the server

### AsyncTcpGateway

- `bind(opts, config)` - Create and start a gateway
- `shutdown()` - Stop the gateway

## Error Handling

All errors are thrown as JavaScript Error objects with descriptive messages:

```javascript
try {
  await client.readHoldingRegisters({ address: 0, count: 10 });
} catch (err) {
  if (err.message.includes('MODBUS_EXCEPTION')) {
    console.error('Modbus exception:', err.message);
  } else if (err.message.includes('MODBUS_TIMEOUT')) {
    console.error('Request timed out');
  } else {
    console.error('Error:', err.message);
  }
}
```

## Supported Platforms

Pre-built binaries are available for:

- Linux (x64, arm64, armv7)
- macOS (x64, arm64)
- Windows (x64, ia32, arm64)

## License

MIT
