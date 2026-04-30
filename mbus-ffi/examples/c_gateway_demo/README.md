# C Gateway Demo

End-to-end demo of the **mbus-ffi C gateway bindings**:

```
+----------------+     TCP     +-----------------------+     TCP     +---------------------+
| Modbus client  |  <------->  | gateway (this binary) |  <------->  | echo Modbus server  |
| (any tool)     |  127.0.0.1  |    upstream:5020      |  127.0.0.1  |       :15020        |
+----------------+   :5020     |    downstream → unit 1|   :15020    +---------------------+
```

The gateway:
1. Spawns a tiny Modbus TCP server in a background thread that responds to FC 0x03 (Read Holding Registers) by returning `register[i] = i`.
2. Wraps a C TCP listener on `127.0.0.1:5020` as the upstream transport (using `MbusTransportCallbacks`).
3. Wraps a C TCP client connected to `127.0.0.1:15020` as a downstream channel.
4. Routes unit ID 1 to channel 0.
5. Calls `mbus_gateway_poll()` in a loop.

## Build

```sh
# 1. Build the Rust library with the c-gateway feature
cargo build -p mbus-ffi --features c-gateway --no-default-features

# 2. Build the C demo
cd mbus-ffi/examples/c_gateway_demo
mkdir -p build && cd build
cmake .. && cmake --build .
```

> **macOS note:** if linking fails with errors referencing `_main` or
> Apple's linker (`ld: warning: ignoring duplicate libraries`), force the
> system clang explicitly:
>
> ```sh
> CC=/usr/bin/clang cmake .. && cmake --build .
> ```
>
> See `mbus-ffi/README.md` for the full toolchain notes.

## Run

```sh
# In one terminal — start the gateway:
./build/c_gateway_demo

# In another terminal — point any Modbus client at 127.0.0.1:5020 / unit 1.
# Example with mbpoll:
mbpoll -m tcp -a 1 -t 4 -r 1 -c 8 -p 5020 127.0.0.1
```

The demo exits after the first complete request/response cycle, or after a 30 s
inactivity timeout.
