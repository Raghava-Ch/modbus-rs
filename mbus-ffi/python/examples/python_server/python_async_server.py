#!/usr/bin/env python3
"""
python_async_server.py — Real-world Modbus TCP + Serial server example using ASYNC/AWAIT.

This script runs a self-contained, fully asynchronous Modbus server that simulates 
a premium industrial controller using modern Python asyncio. 

Key asynchronous features shown here:
  * Non-blocking `async def handle_*` request handlers in ModbusApp.
  * Async database/external service simulator using `asyncio.sleep()`.
  * Coroutine-safe state locking via `asyncio.Lock`.
  * Background device simulation runs as a native asyncio Task on the same event loop.
  * Captures the running event loop seamlessly for Rust's background Tokio execution.

Simulated I/O:
  * 64 coils            (digital outputs — writeable)
  * 64 discrete inputs  (digital inputs  — read-only, auto-toggling)
  * 32 holding registers (process values — writeable)
  * 32 input registers   (sensor readings — read-only, auto-incrementing)
  * FIFO queue           (event log — read-only)
  * Exception status     (bit-packed alarm flags)

# Run:
    # 1. Ensure you are using the virtual environment
    source .venv/bin/activate

    # 2. Build the python extension natively
    cd mbus-ffi 
    maturin develop --features python-server

    # 3. Run the demo script
Run with TCP (default):
    python python_async_server.py
    python python_async_server.py --host 0.0.0.0 --port 5020 --unit-id 1

Run with RTU serial:
    python python_async_server.py --mode serial --port /dev/ttyUSB0 --baud 9600

Stop with Ctrl-C.
"""

import argparse
import asyncio
import logging
import math
import os
import random
import sys
import time
from collections import deque

# ---------------------------------------------------------------------------
# Path bootstrap: auto-discover the workspace .venv so this script runs with
# plain `python3 python_async_server.py` — no `source .venv/bin/activate` needed.
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
log = logging.getLogger("modbus-async-server")

# ─── shared async device state ─────────────────────────────────────────────

class AsyncDeviceState:
    """Coroutine-safe simulated device memory with simulated async I/O latency."""

    COIL_COUNT    = 64
    DI_COUNT      = 64
    HR_COUNT      = 32
    IR_COUNT      = 32
    FIFO_MAX      = 31   # Modbus spec: max 31 words per queue

    def __init__(self):
        # Async Lock for coroutine safety instead of threading.Lock
        self._lock = asyncio.Lock()

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

    async def tick(self):
        """Called periodically by background asyncio task to update simulated sensor readings."""
        async with self._lock:
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

    # ── coroutine-safe async accessors simulating async I/O / DB queries ──────

    async def simulate_db_latency(self):
        """Simulate an asynchronous network request or database query (e.g. 15ms)."""
        await asyncio.sleep(0.015)

    async def read_coils(self, address: int, count: int) -> list[bool]:
        await self.simulate_db_latency()
        async with self._lock:
            return self.coils[address : address + count]

    async def write_coil(self, address: int, value: bool):
        await self.simulate_db_latency()
        async with self._lock:
            self.coils[address] = value
            log.info("coil[%d] ← %s (Async)", address, value)

    async def write_coils(self, address: int, values: list[bool]):
        await self.simulate_db_latency()
        async with self._lock:
            for i, val in enumerate(values):
                self.coils[address + i] = val
            log.info("coils[%d..%d] written (Async)", address, address + len(values) - 1)

    async def read_discrete_inputs(self, address: int, count: int) -> list[bool]:
        await self.simulate_db_latency()
        async with self._lock:
            return self.discrete_inputs[address : address + count]

    async def read_holding(self, address: int, count: int) -> list[int]:
        await self.simulate_db_latency()
        async with self._lock:
            return self.holding[address : address + count]

    async def write_register(self, address: int, value: int):
        await self.simulate_db_latency()
        async with self._lock:
            self.holding[address] = value
            log.info("holding[%d] ← %d (0x%04X) (Async)", address, value, value)

    async def write_registers(self, address: int, values: list[int]):
        await self.simulate_db_latency()
        async with self._lock:
            for i, val in enumerate(values):
                self.holding[address + i] = val
            log.info("holding[%d..%d] written (Async)", address, address + len(values) - 1)

    async def read_input_regs(self, address: int, count: int) -> list[int]:
        await self.simulate_db_latency()
        async with self._lock:
            return self.input_regs[address : address + count]

    async def read_fifo(self) -> list[int]:
        await self.simulate_db_latency()
        async with self._lock:
            return list(self.fifo)

    async def exception_status(self) -> int:
        await self.simulate_db_latency()
        async with self._lock:
            return self.alarms & 0xFF

    async def mask_write_register(self, address: int, and_mask: int, or_mask: int):
        await self.simulate_db_latency()
        async with self._lock:
            current = self.holding[address]
            new_val = (current & and_mask) | (or_mask & ~and_mask)
            self.holding[address] = new_val & 0xFFFF
            log.info("mask_write holding[%d]: AND=0x%04X OR=0x%04X -> 0x%04X (Async)", address, and_mask, or_mask, new_val)

    async def read_write_registers(self, read_address: int, read_count: int, write_address: int, write_values: list[int]) -> list[int]:
        await self.simulate_db_latency()
        async with self._lock:
            for i, val in enumerate(write_values):
                self.holding[write_address + i] = val
            log.info("read_write holding: wrote[%d..%d] and reading[%d..%d] (Async)", 
                     write_address, write_address + len(write_values) - 1,
                     read_address, read_address + read_count - 1)
            return self.holding[read_address : read_address + read_count]


# ─── Modbus application handler with fully ASYNC defs ──────────────────────────

def make_async_app(state: AsyncDeviceState) -> modbus_rs.ModbusApp:
    """Create a ModbusApp that delegates all calls to *state* asynchronously."""

    class IndustrialAsyncApp(modbus_rs.ModbusApp):

        async def handle_read_coils(self, address, count):
            return await state.read_coils(address, count)

        async def handle_write_coil(self, address, value):
            await state.write_coil(address, value)

        async def handle_write_coils(self, address, values):
            await state.write_coils(address, values)

        async def handle_read_discrete_inputs(self, address, count):
            return await state.read_discrete_inputs(address, count)

        async def handle_read_holding_registers(self, address, count):
            return await state.read_holding(address, count)

        async def handle_write_register(self, address, value):
            await state.write_register(address, value)

        async def handle_write_registers(self, address, values):
            await state.write_registers(address, values)

        async def handle_mask_write_register(self, address, and_mask, or_mask):
            await state.mask_write_register(address, and_mask, or_mask)

        async def handle_read_write_registers(self, read_address, read_count, write_address, write_values):
            return await state.read_write_registers(read_address, read_count, write_address, write_values)

        async def handle_read_input_registers(self, address, count):
            return await state.read_input_regs(address, count)

        async def handle_read_fifo_queue(self, pointer_address):
            return await state.read_fifo()

        async def handle_read_exception_status(self):
            return await state.exception_status()

        async def handle_get_comm_event_counter(self):
            # Non-blocking return
            return (0, 0)

    return IndustrialAsyncApp()


# ─── background simulation async task ─────────────────────────────────────────

async def run_simulation(state: AsyncDeviceState):
    log.info("Simulation task started")
    try:
        while True:
            await state.tick()
            await asyncio.sleep(1.0)
    except asyncio.CancelledError:
        log.info("Simulation task cancelled")


# ─── main async application entry ─────────────────────────────────────────────

async def run_async_main(args):
    state = AsyncDeviceState()
    app = make_async_app(state)

    # Start the simulation as a background asyncio task
    sim_task = asyncio.create_task(run_simulation(state))

    try:
        if args.mode == "tcp":
            tcp_port = int(args.port) if args.port else 5020
            log.info("Starting Async Modbus TCP server on %s:%d  unit_id=%d", args.host, tcp_port, args.unit_id)
            server = modbus_rs.AsyncTcpServer(args.host, app, port=tcp_port, unit_id=args.unit_id)
            await server.serve_forever()
        else:
            serial_port = args.port or "/dev/ttyUSB0"
            log.info(
                "Starting Async Modbus Serial server on %s  baud=%d  mode=%s  unit_id=%d",
                serial_port, args.baud, args.serial_mode, args.unit_id,
            )
            server = modbus_rs.AsyncSerialServer(
                serial_port, app, baud_rate=args.baud, unit_id=args.unit_id, mode=args.serial_mode
            )
            await server.serve_forever()
    except asyncio.CancelledError:
        log.info("Server task cancelled")
    finally:
        # Cancel and wait for background simulation task to clean up
        sim_task.cancel()
        try:
            await sim_task
        except asyncio.CancelledError:
            pass


def main():
    parser = argparse.ArgumentParser(description="modbus-rs Python fully async server example")
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

    try:
        asyncio.run(run_async_main(args))
    except KeyboardInterrupt:
        log.info("Server stopped by user via KeyboardInterrupt")


if __name__ == "__main__":
    main()
