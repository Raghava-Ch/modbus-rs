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
# 1. Set up a virtual environment at the repository root
python3 -m venv .venv

# 2. Activate the virtual environment
# On macOS / Linux:
source .venv/bin/activate
# On Windows (Command Prompt):
.venv\Scripts\activate.bat
# On Windows (PowerShell):
.venv\Scripts\Activate.ps1

# (Optional: To deactivate the venv when finished, run: deactivate)

# 3. Install build dependencies
pip install --upgrade pip
pip install maturin pytest pytest-asyncio

# 4. Build/install bindings locally in development mode
cd mbus-ffi
maturin develop --features python-full
```


---

## Quick Start

### Sync TCP client

```python
import modbus_rs

with modbus_rs.TcpTransport.connect("127.0.0.1", port=502) as transport:
    client = transport.create_client(unit_id=1)
    regs = client.read_holding_registers(0, 5)
    print(regs)
```

### Async TCP client

```python
import asyncio
import modbus_rs

async def main():
    async with await modbus_rs.AsyncTcpTransport.connect("127.0.0.1", port=502) as transport:
        client = transport.create_client(unit_id=1)
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

Python bindings support RTU and ASCII serial transports. The transports are opened via `open()` on `RtuTransport` or `AsciiTransport` (or their async counterparts `AsyncRtuTransport` and `AsyncAsciiTransport`), and client instances are then created using the `.create_client(unit_id)` factory method.

Typical serial transport `open()` parameters include:

- `port` (str) — Port path (e.g. `/dev/ttyUSB0` or `COM3`)
- `baud_rate` (int) — Baud rate (default `9600`)
- `timeout_ms` (int) — Timeout in milliseconds (default `1000`)
- `data_bits` (int) — Data bits per character (5/6/7/8, default `8`)
- `parity` (str) — Parity mode (`none`, `even`, or `odd`, default `none`)
- `stop_bits` (int) — Stop bits (1 or 2, default `1`)
- `retry_attempts` (int) — Connection/read retry attempts (default `3`)

Once a transport is created, invoke:
```python
client = transport.create_client(unit_id=1)
```
to get a `SerialModbusClient` for that specific slave device. This design allows multiplexing multiple logic client unit IDs on the same underlying serial transport connection.

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

- mbus-ffi/python/examples/python_client/python_client.py
- mbus-ffi/python/examples/python_async_client/async_client.py
- mbus-ffi/python/examples/python_server/python_server.py

Example guides:

- mbus-ffi/python/examples/python_client/README.md
- mbus-ffi/python/examples/python_async_client/README.md
- mbus-ffi/python/examples/python_server/README.md

### How to run the examples

Before running the examples, ensure your virtual environment is active:

```bash
source .venv/bin/activate
```

Start the server in terminal 1:

```bash
cd mbus-ffi/python/examples/python_server
python3 python_server.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Run the sync client in terminal 2:

```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_client
python3 python_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Run the async client in terminal 2:

```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

Or verify the multi-unit/transport split by running one of the multi-unit examples:

```bash
source .venv/bin/activate
cd mbus-ffi/python/examples
python3 11-tcp-transport-multi-unit.py --host 127.0.0.1 --port 5020
```

Optional multi-server async demo:

```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --multi
```

---

## Testing

Run Python tests from the `mbus-ffi` folder (with your virtual environment active):

```bash
source .venv/bin/activate
cd mbus-ffi
pytest tests/python/ -v
```

Note: Running pytest without installing the extension first will fail collection with an import error for `modbus_rs._modbus_rs`.

---

## Additional Reference

- mbus-ffi/README.md for packaging details and release workflow
- mbus-ffi/src/python for binding source implementation