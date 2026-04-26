"""
test_server.py — Tests for server-side Python bindings.

These tests do not require real hardware.  They spin up a local AsyncTcpServer
on a loopback port and exercise it with a TcpClient.
"""

import asyncio
import os
import shutil
import subprocess
import threading
import time
import uuid

import pytest
import modbus_rs


# ---------------------------------------------------------------------------
# Minimal application used across tests
# ---------------------------------------------------------------------------

class EchoApp(modbus_rs.ModbusApp):
    """Trivial in-memory Modbus application for testing."""

    def __init__(self):
        super().__init__()
        self._coils = [False] * 256
        self._holding = [0] * 256
        self._input = list(range(256))

    def handle_read_coils(self, address, count):
        return self._coils[address : address + count]

    def handle_write_coil(self, address, value):
        self._coils[address] = value

    def handle_write_coils(self, address, count, data):
        for i in range(count):
            byte_idx = i // 8
            bit_idx = i % 8
            if byte_idx < len(data):
                self._coils[address + i] = bool((data[byte_idx] >> bit_idx) & 1)

    def handle_read_discrete_inputs(self, address, count):
        return self._coils[address : address + count]

    def handle_read_holding_registers(self, address, count):
        return self._holding[address : address + count]

    def handle_write_register(self, address, value):
        self._holding[address] = value

    def handle_write_registers(self, address, count, data):
        for i in range(count):
            hi = data[i * 2] if i * 2 < len(data) else 0
            lo = data[i * 2 + 1] if i * 2 + 1 < len(data) else 0
            self._holding[address + i] = (hi << 8) | lo

    def handle_read_input_registers(self, address, count):
        return self._input[address : address + count]

    def handle_read_exception_status(self):
        return 0

    def handle_get_comm_event_counter(self):
        return (0, 0)


# ---------------------------------------------------------------------------
# Helpers — find a free loopback port
# ---------------------------------------------------------------------------

def _free_port() -> int:
    import socket
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def _start_async_tcp_server_in_thread(app, port: int):
    """Start AsyncTcpServer in a daemon thread for test purposes."""
    loop = asyncio.new_event_loop()
    ready = threading.Event()

    async def _run():
        server = modbus_rs.AsyncTcpServer("127.0.0.1", app, port=port, unit_id=1)
        ready.set()
        try:
            await server.serve_forever()
        except Exception:
            pass

    def _thread():
        asyncio.set_event_loop(loop)
        loop.run_until_complete(_run())

    t = threading.Thread(target=_thread, daemon=True)
    t.start()
    ready.wait(timeout=3.0)
    time.sleep(0.1)


@pytest.fixture()
def serial_loopback_ports():
    """Create a virtual serial pair via socat and yield (server_port, client_port)."""
    if shutil.which("socat") is None:
        pytest.skip("socat is required for serial loopback integration tests")

    token = uuid.uuid4().hex[:8]
    server_port = f"/tmp/modbus-ffi-ser-a-{token}"
    client_port = f"/tmp/modbus-ffi-ser-b-{token}"

    cmd = [
        "socat",
        "-d",
        "-d",
        f"pty,raw,echo=0,link={server_port}",
        f"pty,raw,echo=0,link={client_port}",
    ]
    proc = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    deadline = time.time() + 5.0
    while time.time() < deadline:
        if os.path.exists(server_port) and os.path.exists(client_port):
            break
        if proc.poll() is not None:
            stderr = proc.stderr.read() if proc.stderr else ""
            pytest.skip(f"socat exited early; serial loopback unavailable: {stderr}")
        time.sleep(0.05)
    else:
        proc.terminate()
        pytest.skip("timed out waiting for socat loopback PTYs")

    try:
        yield server_port, client_port
    finally:
        if proc.poll() is None:
            proc.terminate()
            try:
                proc.wait(timeout=2)
            except subprocess.TimeoutExpired:
                proc.kill()
        for path in (server_port, client_port):
            try:
                os.unlink(path)
            except FileNotFoundError:
                pass


# ---------------------------------------------------------------------------
# Async server fixture
# ---------------------------------------------------------------------------

@pytest.fixture()
def server_port():
    """Start an AsyncTcpServer in a background thread; yield its port."""
    port = _free_port()
    app = EchoApp()
    loop = asyncio.new_event_loop()
    ready = threading.Event()
    stop = threading.Event()

    async def _run():
        server = modbus_rs.AsyncTcpServer("127.0.0.1", app, port=port, unit_id=1)
        ready.set()
        try:
            await server.serve_forever()
        except Exception:
            pass  # server stops on client disconnect / test teardown

    def _thread():
        asyncio.set_event_loop(loop)
        loop.run_until_complete(_run())

    t = threading.Thread(target=_thread, daemon=True)
    t.start()
    ready.wait(timeout=3.0)
    # Give the OS a moment to bind the socket
    time.sleep(0.1)
    yield port


@pytest.fixture()
def serial_server_ports(serial_loopback_ports):
    """Start AsyncSerialServer on one end of a virtual PTY pair and yield client port."""
    server_port, client_port = serial_loopback_ports
    app = EchoApp()
    loop = asyncio.new_event_loop()
    ready = threading.Event()

    async def _run():
        server = modbus_rs.AsyncSerialServer(
            server_port,
            app,
            baud_rate=9600,
            unit_id=1,
            mode="rtu",
            timeout_ms=1000,
        )
        ready.set()
        try:
            await server.serve_forever()
        except Exception:
            pass

    def _thread():
        asyncio.set_event_loop(loop)
        loop.run_until_complete(_run())

    t = threading.Thread(target=_thread, daemon=True)
    t.start()
    ready.wait(timeout=3.0)
    time.sleep(0.2)

    # Probe whether this host's serial stack supports these virtual PTYs.
    # Some CI/macOS setups expose PTY paths but still fail open/connect.
    probe_err = None
    for _ in range(8):
        try:
            probe = modbus_rs.SerialClient(
                client_port, baud_rate=9600, unit_id=1, mode="rtu", timeout_ms=300
            )
            probe.connect()
            probe_err = None
            break
        except Exception as exc:  # noqa: BLE001 - we want exact skip reason below
            probe_err = exc
            time.sleep(0.1)

    if probe_err is not None:
        pytest.skip(f"serial loopback PTY unsupported in this environment: {probe_err}")

    yield client_port


# ---------------------------------------------------------------------------
# TcpClient integration against EchoApp
# ---------------------------------------------------------------------------

class TestTcpClientIntegration:
    def test_read_holding_registers_all_zeros(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            regs = client.read_holding_registers(0, 5)
        assert regs == [0, 0, 0, 0, 0]

    def test_write_then_read_register(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            client.write_register(10, 0xABCD)
            regs = client.read_holding_registers(10, 1)
        assert regs == [0xABCD]

    def test_read_input_registers(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            regs = client.read_input_registers(0, 4)
        assert regs == [0, 1, 2, 3]

    def test_write_coil_and_read_back(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            client.write_coil(5, True)
            coils = client.read_coils(5, 1)
        assert coils == [True]

    def test_write_multiple_coils(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            client.write_coils(0, [True, False, True])
            coils = client.read_coils(0, 3)
        assert coils == [True, False, True]

    def test_write_multiple_registers(self, server_port):
        with modbus_rs.TcpClient("127.0.0.1", port=server_port, unit_id=1) as client:
            client.connect()
            client.write_registers(20, [1, 2, 3])
            regs = client.read_holding_registers(20, 3)
        assert regs == [1, 2, 3]

    def test_connection_refused_raises(self):
        port = _free_port()  # nothing listening here
        client = modbus_rs.TcpClient("127.0.0.1", port=port, unit_id=1)
        with pytest.raises(modbus_rs.ModbusError):
            client.connect()


# ---------------------------------------------------------------------------
# AsyncTcpClient integration against EchoApp
# ---------------------------------------------------------------------------

@pytest.mark.asyncio
class TestAsyncTcpClientIntegration:
    async def test_read_holding_registers(self, server_port):
        client = modbus_rs.AsyncTcpClient("127.0.0.1", port=server_port, unit_id=1)
        await client.connect()
        regs = await client.read_holding_registers(0, 5)
        assert regs == [0, 0, 0, 0, 0]

    async def test_write_then_read(self, server_port):
        client = modbus_rs.AsyncTcpClient("127.0.0.1", port=server_port, unit_id=1)
        await client.connect()
        await client.write_register(7, 42)
        regs = await client.read_holding_registers(7, 1)
        assert regs == [42]

    async def test_connection_refused_raises(self):
        port = _free_port()
        client = modbus_rs.AsyncTcpClient("127.0.0.1", port=port, unit_id=1)
        with pytest.raises(modbus_rs.ModbusError):
            await client.connect()

    async def test_async_with_returns_client_instance(self, server_port):
        outer = modbus_rs.AsyncTcpClient("127.0.0.1", port=server_port, unit_id=1)
        async with outer as entered:
            assert entered is outer
            regs = await entered.read_holding_registers(0, 1)
        assert regs == [0]


@pytest.mark.asyncio
class TestAsyncServerContextManager:
    async def test_async_tcp_server_aenter_returns_self(self):
        server = modbus_rs.AsyncTcpServer("127.0.0.1", EchoApp(), port=_free_port(), unit_id=1)
        async with server as entered:
            assert entered is server


class TestDispatcherExceptionMapping:
    def test_value_error_maps_to_illegal_data_address(self):
        class ValueErrorApp(modbus_rs.ModbusApp):
            def handle_read_holding_registers(self, address, count):
                raise ValueError("bad range")

        port = _free_port()
        _start_async_tcp_server_in_thread(ValueErrorApp(), port)

        with modbus_rs.TcpClient("127.0.0.1", port=port, unit_id=1) as client:
            client.connect()
            with pytest.raises(modbus_rs.ModbusDeviceException) as exc:
                client.read_holding_registers(0, 1)

        # IllegalDataAddress = 0x02
        assert len(exc.value.args) >= 2
        assert exc.value.args[1] == 0x02


# ---------------------------------------------------------------------------
# Serial integration against loopback PTY pair (binding smoke coverage)
# ---------------------------------------------------------------------------

class TestSerialClientIntegration:
    def test_sync_serial_write_then_read_register(self, serial_server_ports):
        with modbus_rs.SerialClient(
            serial_server_ports, baud_rate=9600, unit_id=1, mode="rtu"
        ) as client:
            client.connect()
            client.write_register(10, 0x1234)
            regs = client.read_holding_registers(10, 1)
        assert regs == [0x1234]

    def test_sync_serial_write_then_read_coil(self, serial_server_ports):
        with modbus_rs.SerialClient(
            serial_server_ports, baud_rate=9600, unit_id=1, mode="rtu"
        ) as client:
            client.connect()
            client.write_coil(3, True)
            coils = client.read_coils(3, 1)
        assert coils == [True]


@pytest.mark.asyncio
class TestAsyncSerialClientIntegration:
    async def test_async_serial_write_then_read_register(self, serial_server_ports):
        outer = modbus_rs.AsyncSerialClient(
            serial_server_ports, baud_rate=9600, unit_id=1, mode="rtu"
        )
        async with outer as client:
            assert client is outer
            await client.write_register(11, 77)
            regs = await client.read_holding_registers(11, 1)
        assert regs == [77]
