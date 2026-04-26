# python_server — Modbus Python Server Example

A self-contained Modbus server that simulates a small industrial I/O device
with live sensor simulation.  Supports both **TCP** and **serial (RTU/ASCII)**
transports.

## Simulated device map

| Object           | Address range | Description                                       |
|------------------|---------------|---------------------------------------------------|
| Coils            | 0 – 63        | Digital outputs (writeable)                       |
| Discrete inputs  | 0 – 63        | Digital inputs (auto-toggling, mirrors coils 0-7) |
| Holding registers | 0 – 31       | `[0]` setpoint × 10 °C, `[1]` output %, …        |
| Input registers  | 0 – 31        | `[0]` temp × 10, `[1]` humidity × 10, `[2]` hPa, `[3]` uptime |
| FIFO queue       | 0             | Event log; code `0x0101` = over-temp alarm        |
| Exception status | —             | Bit 0 = over-temp alarm                           |

## Requirements

**From PyPI (once published):**
```bash
pip install modbus-rs
```

**From source (development build):**
```bash
# From the workspace root
cd /path/to/modbus-rs
python3 -m venv .venv
source .venv/bin/activate        # Windows: .venv\Scripts\activate
cd mbus-ffi
maturin develop --features python,full
```

## Usage

> **Note:** Use `python3` (not `python`; on macOS, `python` often defaults to Python 2.7).
> Scripts have built-in venv auto-discovery, so no activation needed.

**TCP server (default — port 5020):**
```bash
python3 python_server.py
```

**TCP server on a custom host/port:**
```bash
python3 python_server.py --host 0.0.0.0 --port 502 --unit-id 1
```

**RTU serial server:**
```bash
python3 python_server.py --mode serial --port /dev/ttyUSB0 --baud 9600
```

**ASCII serial server:**
```bash
python3 python_server.py --mode serial --port /dev/ttyUSB0 --baud 9600 --serial-mode ascii
```

Stop with **Ctrl-C**.

## Options

| Flag           | Default      | Description                    |
|----------------|--------------|--------------------------------|
| `--mode`       | `tcp`        | `tcp` or `serial`              |
| `--host`       | `0.0.0.0`    | TCP bind address               |
| `--port`       | `5020`       | TCP port or serial device path |
| `--baud`       | `9600`       | Serial baud rate               |
| `--serial-mode`| `rtu`        | `rtu` or `ascii`               |
| `--unit-id`    | `1`          | Modbus unit/slave ID           |

## Quick test with modpoll

```bash
modpoll -t 4:hex -r 0 -c 4 -1 localhost   # read 4 input registers (simulated temp/humidity/pressure/uptime)
```
