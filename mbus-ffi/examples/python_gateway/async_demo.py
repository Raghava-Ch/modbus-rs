"""Async Modbus TCP gateway demo.

Pipeline:

    raw socket client ──▶ AsyncTcpGateway (127.0.0.1:5021) ──▶ AsyncTcpServer

The downstream Modbus server runs on its own asyncio loop in a background
thread to keep the test client free of cross-runtime interference.

Run::

    python mbus-ffi/examples/python_gateway/async_demo.py
"""

from __future__ import annotations

import asyncio
import socket
import struct
import sys
import threading
import time
from contextlib import closing

import modbus_rs


GATEWAY_PORT = 5021
UNIT_ID = 1


class EchoApp(modbus_rs.ModbusApp):
    def handle_read_holding_registers(self, address: int, count: int):
        return [(address + i) & 0xFFFF for i in range(count)]


def free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def wait_port(host: str, port: int, timeout: float = 3.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError(f"port {host}:{port} not listening within {timeout}s")


def start_downstream(port: int) -> tuple[asyncio.AbstractEventLoop, threading.Thread]:
    loop = asyncio.new_event_loop()
    ready = threading.Event()

    async def _run() -> None:
        server = modbus_rs.AsyncTcpServer("127.0.0.1", EchoApp(), port=port, unit_id=UNIT_ID)
        ready.set()
        try:
            await server.serve_forever()
        except asyncio.CancelledError:
            pass
        except Exception as exc:  # noqa: BLE001 — demo logs and exits
            print(f"[downstream] server stopped: {exc!r}", file=sys.stderr)

    def _thread() -> None:
        asyncio.set_event_loop(loop)
        try:
            loop.run_until_complete(_run())
        except RuntimeError:
            pass
        except Exception as exc:  # noqa: BLE001
            print(f"[downstream] event loop error: {exc!r}", file=sys.stderr)

    t = threading.Thread(target=_thread, daemon=True)
    t.start()
    ready.wait(timeout=3.0)
    wait_port("127.0.0.1", port)
    return loop, t


def stop_loop(loop: asyncio.AbstractEventLoop, thread: threading.Thread) -> None:
    loop.call_soon_threadsafe(loop.stop)
    thread.join(timeout=3)


def send_read_holding_registers(host: str, port: int, unit: int, start: int, qty: int) -> bytes:
    with socket.create_connection((host, port), timeout=5) as s:
        req = struct.pack(">HHHBBHH", 1, 0, 6, unit, 3, start, qty)
        s.sendall(req)
        hdr = s.recv(7)
        _txn, _proto, length, _unit = struct.unpack(">HHHB", hdr)
        body = b""
        while len(body) < length - 1:
            chunk = s.recv(length - 1 - len(body))
            if not chunk:
                break
            body += chunk
        return body


async def amain() -> None:
    downstream_port = free_port()
    print(f"Starting downstream Modbus server on 127.0.0.1:{downstream_port} (unit={UNIT_ID})")
    server_loop, server_thread = await asyncio.get_event_loop().run_in_executor(
        None, start_downstream, downstream_port
    )

    print(f"Starting AsyncTcpGateway on 127.0.0.1:{GATEWAY_PORT}")
    gw = modbus_rs.AsyncTcpGateway(f"127.0.0.1:{GATEWAY_PORT}")
    ch = gw.add_tcp_downstream("127.0.0.1", downstream_port)
    gw.add_unit_route(unit=UNIT_ID, channel=ch)

    gw_task = asyncio.ensure_future(gw.serve_forever())
    await asyncio.get_event_loop().run_in_executor(
        None, wait_port, "127.0.0.1", GATEWAY_PORT, 3.0
    )

    try:
        body = await asyncio.get_event_loop().run_in_executor(
            None, send_read_holding_registers, "127.0.0.1", GATEWAY_PORT, UNIT_ID, 0, 5
        )
        regs = struct.unpack(">HHHHH", body[2:12])
        print(f"OK: gateway returned registers {list(regs)} (expected [0, 1, 2, 3, 4])")
    finally:
        gw.stop()
        try:
            await asyncio.wait_for(gw_task, timeout=3)
        except asyncio.TimeoutError:
            gw_task.cancel()
        await asyncio.get_event_loop().run_in_executor(
            None, stop_loop, server_loop, server_thread
        )


if __name__ == "__main__":
    asyncio.run(amain())
