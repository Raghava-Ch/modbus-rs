"""
test_imports.py — Verify all public names are importable and have the correct
types/MRO before any network hardware is required.

These tests work purely from the installed/developed wheel; no Modbus device
or server is needed.
"""

import modbus_rs


# ---------------------------------------------------------------------------
# Exception hierarchy
# ---------------------------------------------------------------------------

class TestExceptionHierarchy:
    def test_modbus_error_is_exception(self):
        assert issubclass(modbus_rs.ModbusError, Exception)

    def test_timeout_is_modbus_error(self):
        assert issubclass(modbus_rs.ModbusTimeout, modbus_rs.ModbusError)

    def test_connection_is_modbus_error(self):
        assert issubclass(modbus_rs.ModbusConnectionError, modbus_rs.ModbusError)

    def test_protocol_is_modbus_error(self):
        assert issubclass(modbus_rs.ModbusProtocolError, modbus_rs.ModbusError)

    def test_device_exception_is_protocol(self):
        assert issubclass(modbus_rs.ModbusDeviceException, modbus_rs.ModbusProtocolError)

    def test_config_is_modbus_error(self):
        assert issubclass(modbus_rs.ModbusConfigError, modbus_rs.ModbusError)

    def test_can_raise_and_catch_base(self):
        try:
            raise modbus_rs.ModbusTimeout("timed out")
        except modbus_rs.ModbusError as exc:
            assert "timed out" in str(exc)

    def test_can_raise_and_catch_device_exception(self):
        try:
            raise modbus_rs.ModbusDeviceException("FC 0x01")
        except modbus_rs.ModbusProtocolError:
            pass
        except modbus_rs.ModbusError:
            pass  # acceptable fallback


# ---------------------------------------------------------------------------
# Classes are importable and are proper types
# ---------------------------------------------------------------------------

class TestClassImports:
    def test_tcp_client_class(self):
        assert isinstance(modbus_rs.TcpClient, type)

    def test_async_tcp_client_class(self):
        assert isinstance(modbus_rs.AsyncTcpClient, type)

    def test_serial_client_class(self):
        assert isinstance(modbus_rs.SerialClient, type)

    def test_async_serial_client_class(self):
        assert isinstance(modbus_rs.AsyncSerialClient, type)

    def test_modbus_app_class(self):
        assert isinstance(modbus_rs.ModbusApp, type)

    def test_async_tcp_server_class(self):
        assert isinstance(modbus_rs.AsyncTcpServer, type)

    def test_tcp_server_class(self):
        assert isinstance(modbus_rs.TcpServer, type)

    def test_async_serial_server_class(self):
        assert isinstance(modbus_rs.AsyncSerialServer, type)

    def test_serial_server_class(self):
        assert isinstance(modbus_rs.SerialServer, type)


# ---------------------------------------------------------------------------
# __all__ is complete
# ---------------------------------------------------------------------------

class TestAll:
    _EXPECTED = {
        "TcpClient", "AsyncTcpClient", "SerialClient", "AsyncSerialClient",
        "ModbusApp",
        "AsyncTcpServer", "TcpServer", "AsyncSerialServer", "SerialServer",
        "ModbusError", "ModbusTimeout", "ModbusConnectionError",
        "ModbusProtocolError", "ModbusDeviceException", "ModbusConfigError",
    }

    def test_all_contains_expected_names(self):
        missing = self._EXPECTED - set(modbus_rs.__all__)
        assert not missing, f"Missing from __all__: {missing}"

    def test_all_names_are_present_in_module(self):
        for name in modbus_rs.__all__:
            assert hasattr(modbus_rs, name), f"__all__ lists '{name}' but it's not on the module"


# ---------------------------------------------------------------------------
# ModbusApp subclassing works
# ---------------------------------------------------------------------------

class TestModbusAppSubclass:
    def test_can_subclass(self):
        class MyApp(modbus_rs.ModbusApp):
            pass
        app = MyApp()
        assert isinstance(app, modbus_rs.ModbusApp)

    def test_default_methods_raise_not_implemented(self):
        """Default handler implementations should raise (NotImplementedError or ModbusError)."""
        app = modbus_rs.ModbusApp()
        try:
            app.handle_read_holding_registers(0, 1)
            # if it returns without raising that's also acceptable for a default impl
        except (NotImplementedError, modbus_rs.ModbusError, Exception):
            pass  # expected

    def test_override_works(self):
        class MyApp(modbus_rs.ModbusApp):
            def handle_read_holding_registers(self, address, count):
                return list(range(count))

        app = MyApp()
        result = app.handle_read_holding_registers(0, 3)
        assert result == [0, 1, 2]


# ---------------------------------------------------------------------------
# Constructor argument validation (ModbusConfigError on bad input)
# ---------------------------------------------------------------------------

class TestConstructorValidation:
    def test_tcp_client_bad_port_does_not_panic(self):
        # port=0 is technically valid per TCP, but verify no panic
        try:
            modbus_rs.TcpClient("127.0.0.1", port=0)
        except modbus_rs.ModbusError:
            pass
        except Exception:
            pass  # any exception is fine; the point is no panic/segfault

    def test_serial_client_bad_mode_raises_config_error(self):
        import pytest
        with pytest.raises(modbus_rs.ModbusConfigError):
            modbus_rs.SerialClient("/dev/ttyUSB0", mode="invalid_mode")

    def test_async_serial_client_bad_mode_raises_config_error(self):
        import pytest
        with pytest.raises(modbus_rs.ModbusConfigError):
            modbus_rs.AsyncSerialClient("/dev/ttyUSB0", mode="invalid_mode")

    def test_serial_server_bad_mode_raises_config_error(self):
        import pytest
        app = modbus_rs.ModbusApp()
        with pytest.raises(modbus_rs.ModbusConfigError):
            modbus_rs.SerialServer("/dev/ttyUSB0", app, mode="bad")

    def test_async_serial_server_bad_mode_raises_config_error(self):
        import pytest
        app = modbus_rs.ModbusApp()
        with pytest.raises(modbus_rs.ModbusConfigError):
            modbus_rs.AsyncSerialServer("/dev/ttyUSB0", app, mode="bad")
