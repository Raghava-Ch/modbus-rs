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

## More Docs

- Project docs: documentation/python_bindings.md
- Full crate README (C/WASM/Python): mbus-ffi/README.md
