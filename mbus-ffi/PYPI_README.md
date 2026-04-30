# modbus-rs (Python)

Fast Modbus TCP + Serial bindings for Python, powered by Rust.

- PyPI package: modbus-rs
- Import name: modbus_rs

## Install

```bash
pip install modbus-rs
```

## Quick Start

### Synchronous TCP client

```python
import modbus_rs

with modbus_rs.TcpClient("192.168.1.10", port=502, unit_id=1) as client:
    client.connect()
    regs = client.read_holding_registers(0, 10)
    print(regs)
```

### Async TCP client

```python
import asyncio
import modbus_rs

async def main():
    async with modbus_rs.AsyncTcpClient("192.168.1.10", unit_id=1) as client:
        regs = await client.read_holding_registers(0, 10)
        print(regs)

asyncio.run(main())
```

### Serial client (RTU)

```python
import modbus_rs

with modbus_rs.SerialClient("/dev/ttyUSB0", baud_rate=9600, unit_id=1) as client:
    client.connect()
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

## Build from Source

```bash
cd mbus-ffi
maturin develop --features python,full
```

## Run Python Tests

```bash
cd mbus-ffi
maturin develop --features python,full
cd ..
pytest mbus-ffi/tests/python/ -q
```

## Run Python Examples

The examples live in this repository under mbus-ffi/examples.

### 1) Build/install the extension from source

```bash
git clone https://github.com/Raghava-Ch/modbus-rs.git
cd modbus-rs/mbus-ffi
maturin develop --features python,full
```

### 2) Start the example server (terminal 1)

```bash
cd ../examples/python_server
python3 python_server.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### 3) Run the sync client (terminal 2)

```bash
cd ../python_client
python3 python_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### 4) Run the async client (terminal 2)

```bash
cd ../python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --unit-id 1
```

### Optional: multi-server async demo

Start 3 servers on ports 5020, 5021, and 5022, then run:

```bash
cd ../python_async_client
python3 async_client.py --host 127.0.0.1 --port 5020 --multi
```

## Modbus TCP Gateway (`python-gateway` feature)

The `python-gateway` feature exposes a thread-safe sync gateway and an
asyncio-friendly async gateway that forward inbound Modbus/TCP requests to
one or more downstream Modbus/TCP servers based on a unit-id routing table.

Build with the gateway feature enabled:

```bash
cd mbus-ffi
maturin develop --features python,python-gateway,full
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

> Note: the optional `event_handler=` constructor argument is reserved for
> future telemetry callbacks. It is currently a forward-compatibility hook and
> does not yet receive frame events.

## More Docs

- Project docs: documentation/python_bindings.md
- Full crate README (C/WASM/Python): mbus-ffi/README.md

## License

Copyright (C) 2026 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](../LICENSE) for details.

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.