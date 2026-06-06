"""
modbus_rs — Python bindings for the modbus-rs library.

PyPI package: modbus-rs
Import name : modbus_rs

Transports and clients
----------------------
TcpTransport            - Sync Modbus TCP transport
AsyncTcpTransport       - Asyncio Modbus TCP transport
RtuTransport            - Sync Modbus serial RTU transport
AsyncRtuTransport       - Asyncio Modbus serial RTU transport
AsciiTransport          - Sync Modbus serial ASCII transport
AsyncAsciiTransport     - Asyncio Modbus serial ASCII transport
TcpModbusClient         - Thin sync Modbus TCP client
AsyncTcpModbusClient    - Thin asyncio Modbus TCP client
SerialModbusClient      - Thin sync Modbus serial client
AsyncSerialModbusClient - Thin asyncio Modbus serial client

Server classes
--------------
ModbusApp       — base class; subclass and override handler methods
AsyncTcpServer  — asyncio Modbus TCP server
TcpServer       — synchronous Modbus TCP server
AsyncSerialServer — asyncio Modbus serial server
SerialServer    — synchronous Modbus serial server

Exceptions
----------
ModbusError           — base exception
ModbusTimeout         — request timed out
ModbusConnectionError — connection failed or lost
ModbusProtocolError   — parse / framing error
ModbusDeviceException — remote device returned a Modbus exception code
ModbusConfigError     — bad constructor arguments
ModbusInvalidArgument — invalid argument passed to a Modbus call
"""

from importlib.metadata import version, PackageNotFoundError

from ._modbus_rs import (
    # clients/transports
    TcpTransport,
    AsyncTcpTransport,
    RtuTransport,
    AsyncRtuTransport,
    AsciiTransport,
    AsyncAsciiTransport,
    TcpModbusClient,
    AsyncTcpModbusClient,
    SerialModbusClient,
    AsyncSerialModbusClient,
    # server
    ModbusApp,
    AsyncTcpServer,
    TcpServer,
    AsyncSerialServer,
    SerialServer,
    # exceptions
    ModbusError,
    ModbusTimeout,
    ModbusConnectionError,
    ModbusProtocolError,
    ModbusDeviceException,
    ModbusConfigError,
    ModbusInvalidArgument,
)

# Gateway classes are only available when the extension is built with the
# `python-gateway` feature.
try:
    from ._modbus_rs import (
        AsyncTcpGateway,
        GatewayEventHandler,
        TcpGateway,
    )
    _GATEWAY_AVAILABLE = True
except ImportError:
    _GATEWAY_AVAILABLE = False

try:
    __version__ = version("modbus-rs")
except PackageNotFoundError:
    __version__ = "0.0.0+unknown"

__all__ = [
    # clients/transports
    "TcpTransport",
    "AsyncTcpTransport",
    "RtuTransport",
    "AsyncRtuTransport",
    "AsciiTransport",
    "AsyncAsciiTransport",
    "TcpModbusClient",
    "AsyncTcpModbusClient",
    "SerialModbusClient",
    "AsyncSerialModbusClient",
    # server
    "ModbusApp",
    "AsyncTcpServer",
    "TcpServer",
    "AsyncSerialServer",
    "SerialServer",
    # exceptions
    "ModbusError",
    "ModbusTimeout",
    "ModbusConnectionError",
    "ModbusProtocolError",
    "ModbusDeviceException",
    "ModbusConfigError",
    "ModbusInvalidArgument",
]

if _GATEWAY_AVAILABLE:
    __all__ += [
        "AsyncTcpGateway",
        "GatewayEventHandler",
        "TcpGateway",
    ]
