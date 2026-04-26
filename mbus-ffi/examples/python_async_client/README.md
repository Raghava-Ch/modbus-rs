# python_async_client — Modbus Asyncio Client Example

Demonstrates the fully-async `AsyncTcpClient` API including:

- Concurrent reads via `asyncio.gather` (read IR + HR + coils + DI + FIFO simultaneously)
- All write operations (`write_register`, `write_registers`, `write_coil`, `mask_write_register`)
- Transparent reconnect loop with exponential back-off
- Multi-server fan-out: poll multiple devices concurrently with a single event loop

## Requirements

**From PyPI (once published):**
```bash
pip install modbus-rs
```

**From source (development build):**
```bash
cd /path/to/modbus-rs
python3 -m venv .venv
source .venv/bin/activate        # Windows: .venv\Scripts\activate
cd mbus-ffi
maturin develop --features python,full
```

## Usage

> **Note:** Use `python3` (not `python`; on macOS, `python` often defaults to Python 2.7).
> Scripts have built-in venv auto-discovery, so no activation needed.

**Start the companion server first:**
```bash
python3 ../python_server/python_server.py
```

**Run the async client (10 iterations, 2 s apart):**
```bash
python3 async_client.py
```

**Custom host / iterations:**
```bash
python3 async_client.py --host 192.168.1.50 --port 502 --unit-id 1 --count 20
```

**Concurrent multi-server demo (opens 3 connections simultaneously):**
```bash
python3 async_client.py --multi
```
*(Requires three servers listening on ports 5020, 5021, 5022 — adjust as needed.)*

## Options

| Flag         | Default     | Description                              |
|--------------|-------------|------------------------------------------|
| `--host`     | `127.0.0.1` | Modbus TCP host                          |
| `--port`     | `5020`      | TCP port                                 |
| `--unit-id`  | `1`         | Modbus unit ID                           |
| `--interval` | `2.0`       | Seconds between polls                    |
| `--count`    | `10`        | Total poll iterations (0 = infinite)     |
| `--multi`    | off         | Fan out to 3 servers concurrently        |

## Key patterns

### Concurrent reads

```python
ir, hr, coils = await asyncio.gather(
    client.read_input_registers(0, 4),
    client.read_holding_registers(0, 4),
    client.read_coils(0, 8),
)
```

All three requests are dispatched at the same time; results arrive in order.

### Context manager (auto-connect / auto-disconnect)

```python
async with modbus_rs.AsyncTcpClient("192.168.1.10", port=502, unit_id=1) as client:
    regs = await client.read_holding_registers(0, 10)
```

### Error handling

```python
try:
    regs = await client.read_holding_registers(0, 10)
except modbus_rs.ModbusTimeout:
    ...   # retryable
except modbus_rs.ModbusDeviceException as exc:
    ...   # device returned an exception code
except modbus_rs.ModbusConnectionError:
    ...   # reconnect
```
