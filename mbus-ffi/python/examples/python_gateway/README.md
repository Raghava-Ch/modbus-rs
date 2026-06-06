# Python Gateway Examples

This directory contains runnable Modbus TCP gateway demos that bridge an
upstream listener (where Modbus clients connect) to a downstream Modbus TCP
server.

## Prerequisites

Virtual environments are not portable and are excluded from git. If you have just cloned the repository, you must create your own virtual environment first:

```bash
# 1. Create a virtual environment (from the repository root)
python3 -m venv .venv

# 2. Activate the virtual environment
# On macOS/Linux:
source .venv/bin/activate
# On Windows:
source .venv/Scripts/activate

# 3. Install the build and test dependencies
pip install maturin pytest pytest-asyncio

# 4. Build and install the python extension natively
cd mbus-ffi 
maturin develop --features python-client,python-gateway
```

## Files

| File | Description |
| ---- | ----------- |
| `sync_demo.py`  | Blocking [`TcpGateway`] running in a worker thread; main thread sends one request through the gateway. |
| `async_demo.py` | Asyncio [`AsyncTcpGateway`] coroutine; raw-socket client validates a request through the gateway. |

## Run

```bash
Run::
    # 1. Ensure you are using the virtual environment
    source .venv/bin/activate

    # 2. Build the python extension natively
    cd mbus-ffi 
    maturin develop --features python-client,python-gateway

    # 3. Run the demo script
    python examples/python_gateway/sync_demo.py
    python examples/python_gateway/async_demo.py
```

Both demos start an in-process downstream Modbus server (`AsyncTcpServer` with
an `EchoApp`) on a free port, then route unit ID 1 to that downstream.

## Notes

* `event_handler` (subclass of `GatewayEventHandler`) is currently accepted by
  the constructors but is not invoked — the underlying async server lacks an
  event-hook surface. The class exists for forward compatibility.
* For a C / `no_std` example see `examples/c_gateway_demo/`.
