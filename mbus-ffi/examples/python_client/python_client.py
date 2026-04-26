#!/usr/bin/env python3
"""
python_client.py — Real-world Modbus TCP + Serial client example.

Connects to a Modbus server and demonstrates:
  * Reading holding / input registers and coils
  * Writing single and multiple registers / coils
  * Mask-write, read-write register operations
  * Reading the FIFO event queue
  * Reading exception status
  * Pretty-printed tabular output
  * Reconnect-on-error retry loop

By default this example targets the companion python_server.py running on
localhost:5020.

Run against the bundled server:
    # Terminal 1
    python python_server.py

    # Terminal 2
    python python_client.py
    python python_client.py --host 192.168.1.50 --port 502 --unit-id 1

Run against a real serial device:
    python python_client.py --mode serial --port /dev/ttyUSB0 --baud 9600
"""

import argparse
import logging
import os
import sys
import time

# ---------------------------------------------------------------------------
# Path bootstrap: auto-discover the workspace .venv so this script runs with
# plain `python3 python_client.py` — no `source .venv/bin/activate` needed.
# Uses site.addsitedir() so that .pth files (used by maturin dev installs)
# are processed correctly.
# ---------------------------------------------------------------------------
def _bootstrap_venv() -> None:
    import os
    import site
    here = os.path.abspath(__file__)
    for _ in range(6):
        here = os.path.dirname(here)
        lib = os.path.join(here, ".venv", "lib")
        if os.path.isdir(lib):
            for entry in os.listdir(lib):
                sp = os.path.join(lib, entry, "site-packages")
                if os.path.isdir(sp):
                    site.addsitedir(sp)
            return

_bootstrap_venv()
del _bootstrap_venv

import modbus_rs

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s [%(levelname)s] %(message)s",
)
log = logging.getLogger("modbus-client")


# ─── helpers ─────────────────────────────────────────────────────────────────

def _bar(value: float, lo: float, hi: float, width: int = 20) -> str:
    """ASCII progress bar normalised to [lo, hi]."""
    fraction = max(0.0, min(1.0, (value - lo) / (hi - lo)))
    filled = round(fraction * width)
    return "[" + "█" * filled + "░" * (width - filled) + "]"


def _bool_list(values: list[bool]) -> str:
    return "  ".join(("ON " if v else "off") for v in values)


# ─── demo routine (sync) ─────────────────────────────────────────────────────

def run_demo_sync(client: modbus_rs.TcpClient | modbus_rs.SerialClient, iteration: int):
    """One pass of the demo — called in a polling loop."""
    sep = "─" * 60

    # ── Input registers (sensor readings) ────────────────────────────────────
    ir = client.read_input_registers(0, 4)
    temp_c  = ir[0] / 10.0
    hum_pct = ir[1] / 10.0
    pressure_hpa = ir[2]
    uptime_s = ir[3]

    print(f"\n{sep}")
    print(f"  Iteration #{iteration}   uptime={uptime_s}s")
    print(sep)
    print(f"  Temperature : {temp_c:6.1f} °C  {_bar(temp_c, 20, 30)}")
    print(f"  Humidity    : {hum_pct:6.1f} %RH {_bar(hum_pct, 40, 60)}")
    print(f"  Pressure    : {pressure_hpa:4d}   hPa {_bar(pressure_hpa, 1000, 1020)}")

    # ── Holding registers (setpoints) ────────────────────────────────────────
    hr = client.read_holding_registers(0, 4)
    setpoint_c = hr[0] / 10.0
    output_pct = hr[1]
    print(f"\n  Setpoint    : {setpoint_c:6.1f} °C  (holding[0])")
    print(f"  Output      : {output_pct:3d} %       (holding[1])")

    # ── Write demo: ramp the setpoint by 0.5 °C each iteration, wrap at 30 ─
    new_sp = round((setpoint_c + 0.5) * 10) % (300 + 1)   # keep ≤ 30.0 °C
    new_sp = new_sp if new_sp >= 200 else 200               # floor 20.0 °C
    addr, written_val = client.write_register(0, new_sp)
    log.info("wrote setpoint %d (%.1f °C) → holding[%d]",
             written_val, written_val / 10.0, addr)

    # Write multiple: set output% to 50, clear spare registers
    client.write_registers(1, [50, 0, 0])

    # ── Coils ─────────────────────────────────────────────────────────────────
    coils = client.read_coils(0, 8)
    print(f"\n  Coils [0-7] : {_bool_list(coils)}")

    # Toggle coil 0 each iteration
    client.write_coil(0, not coils[0])

    # ── Discrete inputs ───────────────────────────────────────────────────────
    di = client.read_discrete_inputs(0, 8)
    print(f"  DI    [0-7] : {_bool_list(di)}")

    # ── Exception status / alarms ─────────────────────────────────────────────
    # The server writes active alarm flags to holding[7] each tick so the
    # client can poll them as a normal holding register.  Bit 0 = over-temp.
    alarm_reg = client.read_holding_registers(7, 1)[0]
    alarm_over_temp = bool(alarm_reg & 0x01)
    print(f"  Over-temp alarm : {'⚠  ACTIVE' if alarm_over_temp else 'OK'}")

    # ── FIFO event queue ──────────────────────────────────────────────────────
    fifo = client.read_fifo_queue(0)
    if fifo:
        print(f"  FIFO events : {[hex(x) for x in fifo]}")
    else:
        print(f"  FIFO events : (empty)")

    # ── Mask-write demo (only every 5 iterations) ─────────────────────────────
    if iteration % 5 == 0:
        # Set bit 8 of holding[2], clear bits 0-3
        addr, _and, _or = client.mask_write_register(2, and_mask=0xFFF0, or_mask=0x0100)
        log.info("mask_write holding[%d]  AND=0xFFF0  OR=0x0100", addr)


# ─── connection factory ───────────────────────────────────────────────────────

def make_tcp_client(host: str, port: int, unit_id: int) -> modbus_rs.TcpClient:
    return modbus_rs.TcpClient(host, port=port, unit_id=unit_id, timeout_ms=2000)


def make_serial_client(
    port: str, baud: int, unit_id: int, mode: str
) -> modbus_rs.SerialClient:
    return modbus_rs.SerialClient(port, baud_rate=baud, unit_id=unit_id,
                                  mode=mode, timeout_ms=2000)


# ─── polling loop ─────────────────────────────────────────────────────────────

def poll_loop(client_factory, interval: float, max_errors: int = 5):
    """Open the client, poll in a loop, reconnect on transient errors."""
    consecutive_errors = 0
    iteration = 0

    while True:
        log.info("Connecting …")
        try:
            client = client_factory()
            client.connect()
            log.info("Connected.")
            consecutive_errors = 0

            while True:
                iteration += 1
                try:
                    run_demo_sync(client, iteration)
                    consecutive_errors = 0
                except modbus_rs.ModbusTimeout:
                    log.warning("Request timed out (iteration %d)", iteration)
                    consecutive_errors += 1
                except modbus_rs.ModbusDeviceException as exc:
                    code = exc.args[1] if len(exc.args) > 1 else None
                    if code == 0x01:
                        log.warning(
                            "Device exception: %s (expected in demo when FC22 mask-write is not implemented by the server app)",
                            exc,
                        )
                    else:
                        log.error("Device exception: %s", exc)
                    consecutive_errors += 1
                except modbus_rs.ModbusConnectionError:
                    log.warning("Connection lost — reconnecting")
                    break

                if consecutive_errors >= max_errors:
                    log.error("Too many consecutive errors; reconnecting")
                    break

                time.sleep(interval)

        except modbus_rs.ModbusConnectionError as exc:
            log.error("Cannot connect: %s", exc)
        except modbus_rs.ModbusConfigError as exc:
            log.error("Configuration error: %s — aborting", exc)
            sys.exit(1)
        except KeyboardInterrupt:
            log.info("Stopped by user")
            return

        log.info("Waiting 3 s before reconnect …")
        try:
            time.sleep(3)
        except KeyboardInterrupt:
            log.info("Stopped by user")
            return


# ─── entry point ─────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="modbus-rs Python client example")
    parser.add_argument("--mode", choices=["tcp", "serial"], default="tcp")
    parser.add_argument("--host", default="127.0.0.1",
                        help="Modbus TCP host (default: 127.0.0.1)")
    parser.add_argument("--port", default=None,
                        help="TCP port (default: 5020) or serial device path")
    parser.add_argument("--baud", type=int, default=9600,
                        help="Serial baud rate (default: 9600)")
    parser.add_argument("--serial-mode", choices=["rtu", "ascii"], default="rtu")
    parser.add_argument("--unit-id", type=int, default=1)
    parser.add_argument("--interval", type=float, default=2.0,
                        help="Polling interval in seconds (default: 2.0)")
    args = parser.parse_args()

    if args.mode == "tcp":
        tcp_port = int(args.port) if args.port else 5020
        factory = lambda: make_tcp_client(args.host, tcp_port, args.unit_id)
    else:
        serial_port = args.port or "/dev/ttyUSB0"
        factory = lambda: make_serial_client(
            serial_port, args.baud, args.unit_id, args.serial_mode
        )

    poll_loop(factory, interval=args.interval)


if __name__ == "__main__":
    main()
