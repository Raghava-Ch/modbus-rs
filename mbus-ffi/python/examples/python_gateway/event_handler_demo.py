"""Async Modbus TCP gateway event handler demo.

This script demonstrates how to subclass `GatewayEventHandler` to receive 
telemetry events from the underlying Rust async gateway server.

Run::
    # 1. Ensure you are using the virtual environment
    source .venv/bin/activate

    # 2. Build the python extension natively
    cd mbus-ffi 
    maturin develop --features python-client,python-gateway

    # 3. Run the demo script
    python examples/python_gateway/event_handler_demo.py
"""

import asyncio
import socket
import struct
import threading
import time
from contextlib import closing

import modbus_rs


GATEWAY_PORT = 5022
UNIT_ID = 1


class LoggingEventHandler(modbus_rs.GatewayEventHandler):
    def on_forward(self, session_id: int, unit_id: int, channel_idx: int) -> None:
        print(f"[EVENT] on_forward: session={session_id}, unit={unit_id}, channel={channel_idx}")

    def on_response_returned(self, session_id: int, upstream_txn: int) -> None:
        print(f"[EVENT] on_response_returned: session={session_id}, txn={upstream_txn}")

    def on_routing_miss(self, session_id: int, unit_id: int) -> None:
        print(f"[EVENT] on_routing_miss: session={session_id}, unit={unit_id}")

    def on_downstream_timeout(self, session_id: int, internal_txn: int) -> None:
        print(f"[EVENT] on_downstream_timeout: session={session_id}, internal_txn={internal_txn}")

    def on_upstream_disconnect(self, session_id: int) -> None:
        print(f"[EVENT] on_upstream_disconnect: session={session_id}")


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

    def _thread() -> None:
        asyncio.set_event_loop(loop)
        try:
            loop.run_until_complete(_run())
        except RuntimeError:
            pass

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
        if len(hdr) < 7:
            return b""
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

    print(f"Starting AsyncTcpGateway on 127.0.0.1:{GATEWAY_PORT} with LoggingEventHandler")
    
    # Pass our custom event handler instance to the gateway
    handler = LoggingEventHandler()
    gw = modbus_rs.AsyncTcpGateway(f"127.0.0.1:{GATEWAY_PORT}", event_handler=handler)
    
    ch = gw.add_tcp_downstream("127.0.0.1", downstream_port)
    gw.add_unit_route(unit=UNIT_ID, channel=ch)

    gw_task = asyncio.ensure_future(gw.serve_forever())
    await asyncio.get_event_loop().run_in_executor(
        None, wait_port, "127.0.0.1", GATEWAY_PORT, 3.0
    )

    try:
        # Send a valid request
        print("\n--- Sending valid request (Unit 1) ---")
        await asyncio.get_event_loop().run_in_executor(
            None, send_read_holding_registers, "127.0.0.1", GATEWAY_PORT, UNIT_ID, 0, 5
        )
        
        # Send a request with a routing miss (Unit 99)
        print("\n--- Sending request with routing miss (Unit 99) ---")
        await asyncio.get_event_loop().run_in_executor(
            None, send_read_holding_registers, "127.0.0.1", GATEWAY_PORT, 99, 0, 5
        )
        
        # Give events a moment to print
        await asyncio.sleep(0.5)
        print("\nDemo complete. Shutting down...")
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
