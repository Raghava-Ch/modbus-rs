"""End-to-end Python gateway tests.

Spins up the modbus_rs.AsyncTcpGateway / TcpGateway against an in-process
downstream Modbus TCP echo server (also in modbus_rs) and exercises a
single Read Holding Registers request through the gateway.
"""

from __future__ import annotations

import asyncio
import socket
import struct
import threading
import time
from contextlib import closing

import pytest

import modbus_rs
from modbus_rs import (
    AsyncTcpGateway,
    AsyncTcpServer,
    GatewayEventHandler,
    ModbusApp,
    TcpGateway,
)


def _free_port() -> int:
    with closing(socket.socket(socket.AF_INET, socket.SOCK_STREAM)) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class EchoApp(ModbusApp):
    """Returns register[i] = (address+i) & 0xFFFF for FC 0x03."""

    def handle_read_holding_registers(self, address: int, count: int):
        return [(address + i) & 0xFFFF for i in range(count)]


def _send_read_holding_registers(
    host: str, port: int, unit: int, start: int, qty: int
) -> bytes:
    """Send a raw Modbus TCP request and return the PDU body bytes."""
    with socket.create_connection((host, port), timeout=20) as s:
        req = struct.pack(">HHHBBHH", 1, 0, 6, unit, 3, start, qty)
        s.sendall(req)
        hdr = s.recv(7)
        assert len(hdr) == 7, "short MBAP header"
        _txn, _proto, length, resp_unit = struct.unpack(">HHHB", hdr)
        assert resp_unit == unit
        body = b""
        while len(body) < length - 1:
            chunk = s.recv(length - 1 - len(body))
            if not chunk:
                break
            body += chunk
        return body


def _wait_port(host: str, port: int, timeout: float = 3.0) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            with socket.create_connection((host, port), timeout=0.2):
                return
        except OSError:
            time.sleep(0.05)
    raise RuntimeError(f"port {host}:{port} not listening within {timeout}s")


def _start_async_tcp_server_in_thread(
    port: int,
) -> tuple[asyncio.AbstractEventLoop, threading.Thread]:
    """Run an EchoApp AsyncTcpServer on a private event loop in a background thread."""
    loop = asyncio.new_event_loop()
    ready = threading.Event()

    async def _run() -> None:
        server = AsyncTcpServer("127.0.0.1", EchoApp(), port=port, unit_id=1)
        ready.set()
        try:
            await server.serve_forever()
        except Exception:
            pass

    def _thread() -> None:
        asyncio.set_event_loop(loop)
        try:
            loop.run_until_complete(_run())
        except Exception:
            pass

    t = threading.Thread(target=_thread, daemon=True)
    t.start()
    ready.wait(timeout=3.0)
    _wait_port("127.0.0.1", port)
    return loop, t


def _stop_loop(loop: asyncio.AbstractEventLoop, thread: threading.Thread) -> None:
    loop.call_soon_threadsafe(loop.stop)
    thread.join(timeout=3)


# ──────────────────────────────────────────────────────────────────────────


def test_module_exports_gateway_classes() -> None:
    assert hasattr(modbus_rs, "AsyncTcpGateway")
    assert hasattr(modbus_rs, "TcpGateway")
    assert hasattr(modbus_rs, "GatewayEventHandler")


def test_invalid_route_raises_before_serve() -> None:
    gw = TcpGateway("127.0.0.1:0")
    with pytest.raises(ValueError):
        gw.add_unit_route(unit=1, channel=0)  # no downstreams
    with pytest.raises(ValueError):
        gw.add_range_route(min=10, max=5, channel=0)


def test_serve_forever_requires_downstream_and_route() -> None:
    gw = TcpGateway("127.0.0.1:0")
    with pytest.raises(ValueError):
        gw.serve_forever()


@pytest.mark.asyncio
async def test_async_gateway_construction_and_stop() -> None:
    """Smoke test: AsyncTcpGateway accepts config and ``stop()`` cleanly aborts."""
    gw = AsyncTcpGateway(f"127.0.0.1:{_free_port()}", event_handler=GatewayEventHandler())
    assert gw.bind_address().startswith("127.0.0.1:")
    ch = gw.add_tcp_downstream("127.0.0.1", _free_port())
    gw.add_unit_route(unit=3, channel=ch)
    gw.add_range_route(min=10, max=20, channel=ch)
    # Stopping before serve_forever() must be a no-op.
    gw.stop()


# ──────────────────────────────────────────────────────────────────────────


def test_sync_gateway_end_to_end() -> None:
    downstream_port = _free_port()
    gateway_port = _free_port()

    server_loop, server_thread = _start_async_tcp_server_in_thread(downstream_port)

    gw = TcpGateway(f"127.0.0.1:{gateway_port}", event_handler=GatewayEventHandler())
    ch = gw.add_tcp_downstream("127.0.0.1", downstream_port)
    gw.add_unit_route(unit=1, channel=ch)

    t_gw = threading.Thread(target=gw.serve_forever, daemon=True)
    t_gw.start()
    _wait_port("127.0.0.1", gateway_port)

    try:
        body = _send_read_holding_registers(
            "127.0.0.1", gateway_port, unit=1, start=10, qty=4
        )
        assert body[0] == 0x03
        assert body[1] == 8
        regs = struct.unpack(">HHHH", body[2:10])
        assert regs == (10, 11, 12, 13)
    finally:
        gw.stop()
        t_gw.join(timeout=3)
        _stop_loop(server_loop, server_thread)


# ──────────────────────────────────────────────────────────────────────────


@pytest.mark.asyncio
@pytest.mark.skip(
    reason=(
        "Skipped under pytest-asyncio: pytest's managed event loop interacts "
        "poorly with pyo3-async-runtimes' tokio bridge, hanging the gateway's "
        "downstream callback. The standalone async demo "
        "(examples/python_gateway/async_demo.py) validates the same path with "
        "asyncio.run() and is run in CI."
    )
)
async def test_async_gateway_end_to_end() -> None:
    downstream_port = _free_port()
    gateway_port = _free_port()

    # Downstream Modbus server lives on its own loop in a background thread.
    server_loop, server_thread = await asyncio.get_event_loop().run_in_executor(
        None, _start_async_tcp_server_in_thread, downstream_port
    )

    gw = AsyncTcpGateway(f"127.0.0.1:{gateway_port}")
    ch = gw.add_tcp_downstream("127.0.0.1", downstream_port)
    gw.add_unit_route(unit=2, channel=ch)
    gw_task = asyncio.ensure_future(gw.serve_forever())

    await asyncio.get_event_loop().run_in_executor(
        None, _wait_port, "127.0.0.1", gateway_port, 3.0
    )

    if gw_task.done():
        gw_task.result()  # surface any startup error

    # Yield to let the tokio runtime fully wire the listener.
    await asyncio.sleep(0.2)

    try:
        body = await asyncio.get_event_loop().run_in_executor(
            None, _send_read_holding_registers, "127.0.0.1", gateway_port, 2, 0, 3
        )
        assert body[0] == 0x03
        assert body[1] == 6
        regs = struct.unpack(">HHH", body[2:8])
        assert regs == (0, 1, 2)
    finally:
        gw.stop()
        try:
            await asyncio.wait_for(gw_task, timeout=3)
        except asyncio.TimeoutError:
            gw_task.cancel()
        await asyncio.get_event_loop().run_in_executor(
            None, _stop_loop, server_loop, server_thread
        )
