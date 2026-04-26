#!/usr/bin/env python3
"""
async_client.py — Asyncio Modbus client example.

Demonstrates the fully async API: concurrent requests across multiple servers,
structured concurrency with asyncio.gather, timeout/retry handling, and
clean cancellation.

Targets the same server as python_client.py (python_server.py on localhost:5020).

Usage:
    # Start the server first:
    python ../python_server/python_server.py

    # Then run this client:
    python async_client.py
    python async_client.py --host 192.168.1.50 --port 502 --unit-id 1 --count 10
"""

import argparse
import asyncio
import logging
import os
import sys
import time

# ---------------------------------------------------------------------------
# Path bootstrap: auto-discover the workspace .venv so this script runs with
# plain `python3 async_client.py` — no `source .venv/bin/activate` needed.
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
log = logging.getLogger("async-client")


# ─── single poll ─────────────────────────────────────────────────────────────

async def poll_once(client: modbus_rs.AsyncTcpClient, label: str = ""):
    """Fire several requests concurrently and pretty-print the results."""
    # Launch all reads at the same time — the async client queues them
    results = await asyncio.gather(
        client.read_input_registers(0, 4),
        client.read_holding_registers(0, 4),
        client.read_coils(0, 8),
        client.read_discrete_inputs(0, 8),
        client.read_fifo_queue(0),
        return_exceptions=True,
    )

    ir, hr, coils, di, fifo = results

    prefix = f"[{label}] " if label else ""

    if isinstance(ir, Exception):
        log.error("%sInput registers error: %s", prefix, ir)
        return False

    temp_c  = ir[0] / 10.0
    hum_pct = ir[1] / 10.0
    pressure = ir[2]
    uptime   = ir[3]

    setpoint_c = hr[0] / 10.0 if not isinstance(hr, Exception) else "?"
    output_pct = hr[1] if not isinstance(hr, Exception) else "?"

    log.info(
        "%sT=%.1f°C  H=%.1f%%  P=%dhPa  uptime=%ds  sp=%.1f°C  out=%s%%",
        prefix, temp_c, hum_pct, pressure, uptime,
        setpoint_c, output_pct,
    )
    if not isinstance(coils, Exception):
        log.info("%sCoils: %s", prefix,
                 " ".join("1" if c else "0" for c in coils))
    if not isinstance(fifo, Exception) and fifo:
        log.info("%sFIFO events: %s", prefix, [hex(x) for x in fifo])

    return True


# ─── write demo ──────────────────────────────────────────────────────────────

async def write_demo(client: modbus_rs.AsyncTcpClient):
    """Demonstrate various write operations."""
    # Read current setpoint
    hr = await client.read_holding_registers(0, 1)
    current_sp = hr[0]

    # Bump setpoint by 1 (0.1 °C)
    new_sp = (current_sp + 1) % 301
    new_sp = max(new_sp, 200)
    addr, val = await client.write_register(0, new_sp)
    log.info("setpoint → %.1f°C (holding[%d] = %d)", val / 10.0, addr, val)

    # Write multiple registers: output%, spare, spare
    await client.write_registers(1, [60, 0, 0])

    # Toggle coil 1
    coils = await client.read_coils(1, 1)
    await client.write_coil(1, not coils[0])

    # Mask-write holding[2]: set bit 0, clear bits 4-7
    addr, a, o = await client.mask_write_register(2, and_mask=0xFF0F, or_mask=0x0001)
    log.info("mask_write holding[%d]: AND=0xFF0F OR=0x0001", addr)


# ─── concurrent multi-server demo ────────────────────────────────────────────

async def run_concurrent(hosts: list[tuple[str, int]], unit_id: int, count: int):
    """
    Connect to multiple servers and poll them concurrently.
    Each host gets its own AsyncTcpClient task.
    """
    async def per_server(host: str, port: int, label: str):
        try:
            async with modbus_rs.AsyncTcpClient(
                host, port=port, unit_id=unit_id, timeout_ms=2000
            ) as client:
                for i in range(count):
                    ok = await poll_once(client, label=label)
                    if not ok:
                        log.warning("[%s] polling failed on iteration %d", label, i)
                    if i == 0:
                        await write_demo(client)
                    await asyncio.sleep(2.0)
        except modbus_rs.ModbusConnectionError as exc:
            log.error("[%s] Connection failed: %s", label, exc)
        except modbus_rs.ModbusError as exc:
            log.error("[%s] Modbus error: %s", label, exc)

    tasks = [
        asyncio.create_task(per_server(host, port, f"{host}:{port}"))
        for host, port in hosts
    ]
    await asyncio.gather(*tasks)


# ─── reconnect loop ───────────────────────────────────────────────────────────

async def poll_with_reconnect(
    host: str, port: int, unit_id: int, interval: float, count: int
):
    """Reconnect transparently if the connection drops."""
    iteration = 0
    while iteration < count:
        try:
            log.info("Connecting to %s:%d …", host, port)
            async with modbus_rs.AsyncTcpClient(
                host, port=port, unit_id=unit_id, timeout_ms=2000
            ) as client:
                log.info("Connected.")
                while iteration < count:
                    ok = await poll_once(client, label=f"#{iteration + 1}")
                    if ok:
                        await write_demo(client)
                    iteration += 1
                    if iteration < count:
                        await asyncio.sleep(interval)
        except modbus_rs.ModbusConnectionError as exc:
            log.warning("Connection lost (%s) — retry in 3 s", exc)
            await asyncio.sleep(3.0)
        except modbus_rs.ModbusTimeout:
            log.warning("Timeout on iteration %d — retrying", iteration)
            await asyncio.sleep(1.0)


# ─── entry point ─────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="modbus-rs async client example")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=5020)
    parser.add_argument("--unit-id", type=int, default=1)
    parser.add_argument("--interval", type=float, default=2.0,
                        help="Seconds between polls (default: 2.0)")
    parser.add_argument("--count", type=int, default=10,
                        help="Number of poll iterations (default: 10)")
    parser.add_argument("--multi", action="store_true",
                        help="Demo: connect to 3 copies of the server simultaneously")
    args = parser.parse_args()

    try:
        if args.multi:
            # For the multi-server demo we just fan out to the same host
            # with slightly different ports — adjust for your environment.
            hosts = [
                (args.host, args.port),
                (args.host, args.port + 1),
                (args.host, args.port + 2),
            ]
            asyncio.run(run_concurrent(hosts, args.unit_id, args.count))
        else:
            asyncio.run(
                poll_with_reconnect(
                    args.host, args.port, args.unit_id, args.interval, args.count
                )
            )
    except KeyboardInterrupt:
        log.info("Stopped by user")


if __name__ == "__main__":
    main()
