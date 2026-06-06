# modbus-rs (Python)

Fast Modbus TCP + Serial bindings for Python, powered by Rust.

- PyPI package: modbus-rs
- Import name: modbus_rs

## Licensing

This package is available under GNU GPL v3.0 for open-source use.

Commercial licenses are also available for proprietary/closed-source use.
Contact: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)

## Install

```bash
pip install modbus-rs
```

## Quick Start

Example codes: [modbus-rs/blob/main/mbus-ffi/python/examples](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/python/examples)

### Synchronous TCP client

```python
import modbus_rs

with modbus_rs.TcpTransport.connect("192.168.1.10", port=502) as transport:
    client = transport.create_client(unit_id=1)
    regs = client.read_holding_registers(0, 10)
    print(regs)
```

### Async TCP client

```python
import asyncio
import modbus_rs

async def main():
    async with await modbus_rs.AsyncTcpTransport.connect("192.168.1.10") as transport:
        client = transport.create_client(unit_id=1)
        regs = await client.read_holding_registers(0, 10)
        print(regs)

asyncio.run(main())
```

### Serial client (RTU)

```python
import modbus_rs

with modbus_rs.RtuTransport.open("/dev/ttyUSB0", baud_rate=9600) as transport:
    client = transport.create_client(unit_id=1)
    regs = client.read_holding_registers(0, 5)
    print(regs)
```

### Async TCP server

```python
import asyncio
import modbus_rs

class MyApp(modbus_rs.ModbusApp):
    def handle_read_holding_registers(self, address, count):
        return [address + i for i in range(count)]

async def main():
    server = modbus_rs.AsyncTcpServer("0.0.0.0", MyApp(), port=5020, unit_id=1)
    await server.serve_forever()

asyncio.run(main())
```

## Exceptions

- ModbusError
- ModbusTimeout
- ModbusConnectionError
- ModbusProtocolError
- ModbusDeviceException
- ModbusConfigError
- ModbusInvalidArgument

## Build and Test Locally

To develop the Python bindings locally, create a virtual environment, activate it, build the bindings using Maturin, and run the tests.

### 1) Set up a virtual environment
Create a Python virtual environment at the repository root to isolate dependencies:
```bash
# Create the virtual environment
python3 -m venv .venv

# Activate the virtual environment
# On macOS / Linux:
source .venv/bin/activate
# On Windows (Command Prompt):
.venv\Scripts\activate.bat
# On Windows (PowerShell):
.venv\Scripts\Activate.ps1
```

Once activated, your terminal prompt will be prefixed with `(.venv)`. To deactivate the virtual environment when you are done, run:
```bash
deactivate
```

### 2) Install build/test dependencies
```bash
pip install --upgrade pip
pip install maturin pytest pytest-asyncio
```

### 3) Compile and install the bindings in development mode
From the repository root, change to the `mbus-ffi` directory and compile the package.

The Python bindings features are modular:
- `python-client` — Enables Modbus client transports and clients.
- `python-server` — Enables Modbus server classes and apps.
- `python-gateway` — Enables TCP gateway classes (requires `python-client`).
- `python-full` — Convenience alias that enables all client, server, and gateway features.

To compile with all features enabled:
```bash
cd mbus-ffi
maturin develop --features python-full
```

### 4) Run Python tests
Run pytest:
```bash
pytest tests/python/ -v
```

## Run Python Examples

The examples live in this repository under `mbus-ffi/python/examples/`. Before running them, make sure your virtual environment is activated and the extension is built.

### 1) Build/install the extension from source
Ensure you are in the repository root, activate the virtual environment, and compile the package:
```bash
source .venv/bin/activate   # or Windows equivalent
cd mbus-ffi
maturin develop --features python-full
```

### 2) Start the example server (terminal 1)
Run the server from the repository root (make sure the virtual environment is active):
```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_server
python3 python_server.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### 3) Run the sync client (terminal 2)
Run the client from the repository root:
```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_client
python3 python_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### 4) Run the async client (terminal 2)
Run the async client from the repository root:
```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### 5) Run multi-unit examples (terminal 2)
Verify the new transport/client split by running one of the multi-unit/transport examples from the repository root:
```bash
source .venv/bin/activate
cd mbus-ffi/python/examples
python3 11-tcp-transport-multi-unit.py --host 127.0.0.1 --port 5020
```

### Optional: multi-server async demo
Start 3 servers on ports 5020, 5021, and 5022, then run:
```bash
source .venv/bin/activate
cd mbus-ffi/python/examples/python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --multi
```

## Modbus TCP Gateway (`python-gateway` feature)

The `python-gateway` feature exposes a thread-safe sync gateway and an
asyncio-friendly async gateway that forward inbound Modbus/TCP requests to
one or more downstream Modbus/TCP servers based on a unit-id routing table.

Build with the gateway feature enabled (or use the complete `python-full` suite):

```bash
cd mbus-ffi
maturin develop --features python-client,python-gateway
```

### Sync gateway

```python
import modbus_rs

gw = modbus_rs.TcpGateway("0.0.0.0:5020")
ch = gw.add_tcp_downstream("192.168.1.10", 502)
gw.add_unit_route(unit=1, channel=ch)
gw.serve_forever()  # blocks; call gw.stop() from another thread to exit
```

### Async gateway

```python
import asyncio
import modbus_rs

async def main():
    gw = modbus_rs.AsyncTcpGateway("0.0.0.0:5020")
    ch = await gw.add_tcp_downstream("192.168.1.10", 502)
    await gw.add_unit_route(unit=1, channel=ch)
    await gw.serve_forever()  # cancel the task or call gw.stop() to exit

asyncio.run(main())
```

> Note: The optional `event_handler=` constructor argument accepts a `GatewayEventHandler` subclass to receive telemetry callbacks for routing, forwarding, and errors. See [event_handler_demo.py](https://github.com/Raghava-Ch/modbus-rs/blob/main/mbus-ffi/examples/python_gateway/event_handler_demo.py) for a complete example of logging telemetry events.

## More Docs

- Project docs: documentation/python_bindings.md
- Full crate README (C/WASM/Python): mbus-ffi/README.md

## License

Copyright (C) 2026 Raghava Challari

This project is licensed under GNU GPL v3.0.
See [LICENSE](../LICENSE) for details.

Commercial licenses for proprietary use are available via [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com).