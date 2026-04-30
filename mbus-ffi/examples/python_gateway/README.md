# Python Gateway Examples

This directory contains runnable Modbus TCP gateway demos that bridge an
upstream listener (where Modbus clients connect) to a downstream Modbus TCP
server.

## Prerequisites

Build and install the extension with the gateway feature enabled:

```bash
cd mbus-ffi
maturin develop --features python,python-gateway,full
```

## Files

| File | Description |
| ---- | ----------- |
| `sync_demo.py`  | Blocking [`TcpGateway`] running in a worker thread; main thread sends one request through the gateway. |
| `async_demo.py` | Asyncio [`AsyncTcpGateway`] coroutine; raw-socket client validates a request through the gateway. |

## Run

```bash
python mbus-ffi/examples/python_gateway/sync_demo.py
python mbus-ffi/examples/python_gateway/async_demo.py
```

Both demos start an in-process downstream Modbus server (`AsyncTcpServer` with
an `EchoApp`) on a free port, then route unit ID 1 to that downstream.

## Notes

* `event_handler` (subclass of `GatewayEventHandler`) is currently accepted by
  the constructors but is not invoked — the underlying async server lacks an
  event-hook surface. The class exists for forward compatibility.
* For a C / `no_std` example see `examples/c_gateway_demo/`.
