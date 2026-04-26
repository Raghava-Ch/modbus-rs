# Python Bindings

This page covers Python usage for the modbus-rs stack through the mbus-ffi crate.

Package name on PyPI: modbus-rs  
Import name in Python: modbus_rs

---

## Install

```bash
pip install modbus-rs
```

For local development from this repository:

```bash
cd mbus-ffi
maturin develop --features python,full
```

---

## Quick Start

### Sync TCP client

```python
import modbus_rs

with modbus_rs.TcpClient("127.0.0.1", port=502, unit_id=1) as client:
    client.connect()
    regs = client.read_holding_registers(0, 5)
    print(regs)
```

### Async TCP client

```python
import asyncio
import modbus_rs

async def main():
    async with modbus_rs.AsyncTcpClient("127.0.0.1", port=502, unit_id=1) as client:
        regs = await client.read_holding_registers(0, 5)
        print(regs)

asyncio.run(main())
```

### Async TCP server

```python
import asyncio
import modbus_rs

class App(modbus_rs.ModbusApp):
    def handle_read_holding_registers(self, address, count):
        return [address + i for i in range(count)]

async def main():
    server = modbus_rs.AsyncTcpServer("0.0.0.0", App(), port=5020, unit_id=1)
    await server.serve_forever()

asyncio.run(main())
```

---

## Serial Usage

Python bindings support RTU and ASCII serial transports.

Typical serial constructor parameters include:

- port
- baud_rate
- unit_id
- mode (rtu or ascii, or SerialMode enum)
- timeout_ms
- data_bits
- parity
- stop_bits
- retry_attempts

---

## Exceptions

Primary exception hierarchy:

- ModbusError
- ModbusTimeout
- ModbusConnectionError
- ModbusProtocolError
- ModbusDeviceException
- ModbusConfigError
- ModbusInvalidArgument

Use ModbusDeviceException when handling device-returned Modbus exception codes.

---

## Examples

Complete runnable examples:

- mbus-ffi/examples/python_client/python_client.py
- mbus-ffi/examples/python_async_client/async_client.py
- mbus-ffi/examples/python_server/python_server.py

Example guides:

- mbus-ffi/examples/python_client/README.md
- mbus-ffi/examples/python_async_client/README.md
- mbus-ffi/examples/python_server/README.md

### How to run the examples

From repository root:

```bash
cd mbus-ffi
maturin develop --features python,full
```

Start the server in terminal 1:

```bash
cd mbus-ffi/examples/python_server
python3 python_server.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Run the sync client in terminal 2:

```bash
cd mbus-ffi/examples/python_client
python3 python_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Run the async client in terminal 2:

```bash
cd mbus-ffi/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Optional multi-server async demo:

```bash
cd mbus-ffi/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --multi
```

---

## Testing

Run Python tests from repository root:

```bash
cd mbus-ffi && maturin develop --features python,full
cd ..
pytest mbus-ffi/tests/python/ -q
```

Note: Running pytest without installing the extension first will fail collection with an import error for modbus_rs._modbus_rs.

---

## Additional Reference

- mbus-ffi/README.md for packaging details and release workflow
- mbus-ffi/src/python for binding source implementation