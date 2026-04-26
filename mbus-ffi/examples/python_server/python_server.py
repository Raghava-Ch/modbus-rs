#!/usr/bin/env python3
"""
python_server.py — Real-world Modbus TCP + Serial server example.

This script runs a self-contained Modbus server that simulates a small
industrial I/O device with:

  * 64 coils            (digital outputs — writeable)
  * 64 discrete inputs  (digital inputs  — read-only, auto-toggling)
  * 32 holding registers (process values — writeable)
  * 32 input registers   (sensor readings — read-only, auto-incrementing)
  * FIFO queue           (event log — read-only)
  * Exception status     (bit-packed alarm flags)

Run with TCP (default):
    python python_server.py
    python python_server.py --host 0.0.0.0 --port 5020 --unit-id 1

Run with RTU serial:
    python python_server.py --mode serial --port /dev/ttyUSB0 --baud 9600

Stop with Ctrl-C.
"""

import argparse
import asyncio
import logging
import math
import os
import random
import sys
import threading
import time
from collections import deque

# ---------------------------------------------------------------------------
# Path bootstrap: auto-discover the workspace .venv so this script runs with
# plain `python3 python_server.py` — no `source .venv/bin/activate` needed.
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
    format="%(asctime)s [%(levelname)s] %(name)s: %(message)s",
)
log = logging.getLogger("modbus-server")

# ─── shared device state ─────────────────────────────────────────────────────

class DeviceState:
    """Thread-safe simulated device memory."""

    COIL_COUNT    = 64
    DI_COUNT      = 64
    HR_COUNT      = 32
    IR_COUNT      = 32
    FIFO_MAX      = 31   # Modbus spec: max 31 words per queue

    def __init__(self):
        self._lock = threading.Lock()

        # Coils: digital outputs (writeable)
        self.coils = [False] * self.COIL_COUNT

        # Discrete inputs: digital inputs (read-only)
        self.discrete_inputs = [False] * self.DI_COUNT

        # Holding registers: 16-bit process values (writeable)
        # 0: setpoint (°C * 10)   1: output %   2-7: spare
        self.holding = [0] * self.HR_COUNT
        self.holding[0] = 250   # default setpoint: 25.0 °C

        # Input registers: 16-bit sensor readings (read-only)
        # 0: temperature * 10 (°C)   1: humidity * 10 (%RH)
        # 2: pressure (hPa)          3: uptime (s, lower 16 bits)
        self.input_regs = [0] * self.IR_COUNT

        # FIFO event log
        self.fifo: deque[int] = deque(maxlen=self.FIFO_MAX)

        # Alarm flags (bit-packed into exception status)
        self.alarms: int = 0

        self._tick = 0

    # ── simulation tick ───────────────────────────────────────────────────────

    def tick(self):
        """Called every second to update simulated sensor readings."""
        with self._lock:
            self._tick += 1
            t = self._tick

            # Sinusoidal temperature: 20 – 30 °C
            temp_c = 25.0 + 5.0 * math.sin(t / 30.0)
            self.input_regs[0] = round(temp_c * 10) & 0xFFFF

            # Random humidity: 40 – 60 %RH
            hum = 50.0 + 10.0 * math.sin(t / 20.0 + 1.0)
            self.input_regs[1] = round(hum * 10) & 0xFFFF

            # Slowly rising pressure: 1000 – 1020 hPa
            pressure = 1010 + 10 * math.sin(t / 60.0)
            self.input_regs[2] = round(pressure) & 0xFFFF

            # Uptime (lower 16 bits of tick counter)
            self.input_regs[3] = t & 0xFFFF

            # Toggle first 4 discrete inputs based on tick parity
            for i in range(4):
                self.discrete_inputs[i] = bool((t >> i) & 1)

            # Mirror coil states to discrete inputs 8–15
            for i in range(8):
                self.discrete_inputs[8 + i] = self.coils[i]

            # Alarm: over-temperature if temp > 28 °C
            if temp_c > 28.0:
                self.alarms |= 0x01
                if not self.fifo or self.fifo[-1] != 0x0101:
                    self.fifo.append(0x0101)   # event code: over-temp
            else:
                self.alarms &= ~0x01

            # Mirror alarm flags into holding[7] so clients can poll them
            self.holding[7] = self.alarms & 0xFF

    # ── thread-safe accessors ─────────────────────────────────────────────────

    def read_coils(self, address: int, count: int) -> list[bool]:
        with self._lock:
            return self.coils[address : address + count]

    def write_coil(self, address: int, value: bool):
        with self._lock:
            self.coils[address] = value
            log.info("coil[%d] ← %s", address, value)

    def write_coils(self, address: int, count: int, data: bytes):
        with self._lock:
            for i in range(count):
                byte_idx, bit_idx = divmod(i, 8)
                if byte_idx < len(data):
                    self.coils[address + i] = bool((data[byte_idx] >> bit_idx) & 1)
            log.info("coils[%d..%d] written", address, address + count - 1)

    def read_discrete_inputs(self, address: int, count: int) -> list[bool]:
        with self._lock:
            return self.discrete_inputs[address : address + count]

    def read_holding(self, address: int, count: int) -> list[int]:
        with self._lock:
            return self.holding[address : address + count]

    def write_register(self, address: int, value: int):
        with self._lock:
            self.holding[address] = value
            log.info("holding[%d] ← %d (0x%04X)", address, value, value)

    def write_registers(self, address: int, count: int, data: bytes):
        with self._lock:
            for i in range(count):
                hi = data[i * 2] if i * 2 < len(data) else 0
                lo = data[i * 2 + 1] if i * 2 + 1 < len(data) else 0
                self.holding[address + i] = (hi << 8) | lo
            log.info("holding[%d..%d] written", address, address + count - 1)

    def read_input_regs(self, address: int, count: int) -> list[int]:
        with self._lock:
            return self.input_regs[address : address + count]

    def read_fifo(self) -> list[int]:
        with self._lock:
            return list(self.fifo)

    def exception_status(self) -> int:
        with self._lock:
            return self.alarms & 0xFF


# ─── Modbus application handler ───────────────────────────────────────────────

def make_app(state: DeviceState) -> modbus_rs.ModbusApp:
    """Create a ModbusApp that delegates all calls to *state*."""

    class IndustrialIOApp(modbus_rs.ModbusApp):

        def handle_read_coils(self, address, count):
            return state.read_coils(address, count)

        def handle_write_coil(self, address, value):
            state.write_coil(address, value)

        def handle_write_coils(self, address, count, data):
            state.write_coils(address, count, bytes(data))

        def handle_read_discrete_inputs(self, address, count):
            return state.read_discrete_inputs(address, count)

        def handle_read_holding_registers(self, address, count):
            return state.read_holding(address, count)

        def handle_write_register(self, address, value):
            state.write_register(address, value)

        def handle_write_registers(self, address, count, data):
            state.write_registers(address, count, bytes(data))

        def handle_read_input_registers(self, address, count):
            return state.read_input_regs(address, count)

        def handle_read_fifo_queue(self, pointer_address):
            # pointer_address is the FIFO queue pointer (ignored here — one queue)
            return state.read_fifo()

        def handle_read_exception_status(self):
            return state.exception_status()

        def handle_get_comm_event_counter(self):
            return (0, 0)

    return IndustrialIOApp()


# ─── background simulation thread ────────────────────────────────────────────

def run_simulation(state: DeviceState, stop_event: threading.Event):
    log.info("Simulation started")
    while not stop_event.is_set():
        state.tick()
        stop_event.wait(timeout=1.0)
    log.info("Simulation stopped")


# ─── TCP server ───────────────────────────────────────────────────────────────

async def run_tcp_server(host: str, port: int, unit_id: int, state: DeviceState):
    app = make_app(state)
    log.info("Starting Modbus TCP server on %s:%d  unit_id=%d", host, port, unit_id)
    server = modbus_rs.AsyncTcpServer(host, app, port=port, unit_id=unit_id)
    await server.serve_forever()


# ─── Serial server ────────────────────────────────────────────────────────────

async def run_serial_server(
    port: str, baud: int, unit_id: int, mode: str, state: DeviceState
):
    app = make_app(state)
    log.info(
        "Starting Modbus Serial server on %s  baud=%d  mode=%s  unit_id=%d",
        port, baud, mode, unit_id,
    )
    server = modbus_rs.AsyncSerialServer(
        port, app, baud_rate=baud, unit_id=unit_id, mode=mode
    )
    await server.serve_forever()


# ─── entry point ─────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="modbus-rs Python server example")
    parser.add_argument("--mode", choices=["tcp", "serial"], default="tcp",
                        help="Transport mode (default: tcp)")
    parser.add_argument("--host", default="0.0.0.0",
                        help="TCP bind address (default: 0.0.0.0)")
    parser.add_argument("--port", default=None,
                        help="TCP port (default: 5020) or serial device path")
    parser.add_argument("--baud", type=int, default=9600,
                        help="Serial baud rate (default: 9600)")
    parser.add_argument("--serial-mode", choices=["rtu", "ascii"], default="rtu",
                        help="RTU or ASCII framing (default: rtu)")
    parser.add_argument("--unit-id", type=int, default=1,
                        help="Modbus unit/slave ID (default: 1)")
    args = parser.parse_args()

    state = DeviceState()
    stop_event = threading.Event()

    sim_thread = threading.Thread(
        target=run_simulation, args=(state, stop_event), daemon=True
    )
    sim_thread.start()

    loop = asyncio.new_event_loop()
    asyncio.set_event_loop(loop)

    try:
        if args.mode == "tcp":
            tcp_port = int(args.port) if args.port else 5020
            loop.run_until_complete(
                run_tcp_server(args.host, tcp_port, args.unit_id, state)
            )
        else:
            serial_port = args.port or "/dev/ttyUSB0"
            loop.run_until_complete(
                run_serial_server(
                    serial_port, args.baud, args.unit_id, args.serial_mode, state
                )
            )
    except KeyboardInterrupt:
        log.info("Server stopped by user")
    finally:
        stop_event.set()
        sim_thread.join(timeout=2)
        loop.close()


if __name__ == "__main__":
    main()
