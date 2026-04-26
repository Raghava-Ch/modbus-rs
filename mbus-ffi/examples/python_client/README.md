# python_client — Modbus Python Sync Client Example

A synchronous (blocking) Modbus client that polls a server in a loop,
demonstrates all major read/write operations, and handles reconnects
automatically.

Designed to target the companion `python_server` example, but works against
any Modbus TCP/serial device.

## What it demonstrates

- `read_input_registers` — temperature, humidity, pressure, uptime
- `read_holding_registers` / `write_register` / `write_registers` — setpoint + output
- `read_coils` / `write_coil` / `write_coils` — digital outputs
- `read_discrete_inputs` — digital inputs
- `read_fifo_queue` — event log
- `mask_write_register` — atomic bit manipulation
- Reconnect-on-error retry loop
- Pretty-printed ASCII bar charts

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

**Poll the local example server (start `python_server.py` first):**
```bash
python3 python_client.py
```

**Poll a remote TCP device:**
```bash
python3 python_client.py --host 192.168.1.50 --port 502 --unit-id 1
```

**Poll a serial device:**
```bash
python3 python_client.py --mode serial --port /dev/ttyUSB0 --baud 9600 --unit-id 1
```

## Options

| Flag            | Default       | Description                        |
|-----------------|---------------|------------------------------------|
| `--mode`        | `tcp`         | `tcp` or `serial`                  |
| `--host`        | `127.0.0.1`   | Modbus TCP host                    |
| `--port`        | `5020`        | TCP port or serial device path     |
| `--baud`        | `9600`        | Serial baud rate                   |
| `--serial-mode` | `rtu`         | `rtu` or `ascii`                   |
| `--unit-id`     | `1`           | Modbus unit/slave ID               |
| `--interval`    | `2.0`         | Polling interval (seconds)         |

## Sample output

```
──────────────────────────────────────────────────────────
  Iteration #3   uptime=12s
──────────────────────────────────────────────────────────
  Temperature : 26.3 °C  [███████████░░░░░░░░░]
  Humidity    : 51.2 %RH [██████████░░░░░░░░░░]
  Pressure    : 1012 hPa [████████░░░░░░░░░░░░]

  Setpoint    :   25.5 °C  (holding[0])
  Output      :  50 %       (holding[1])

  Coils [0-7] : ON   off  off  off  off  off  off  off
  DI    [0-7] : ON   off  ON   off  ON   off  off  off
  Over-temp alarm : OK
  FIFO events : (empty)
```
