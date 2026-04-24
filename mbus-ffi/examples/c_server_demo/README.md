# c_server_demo

Minimal hand-written C demo for the `mbus-ffi` server FFI.  No YAML codegen — all
handler wiring is written directly in `main.c`.

What it demonstrates:
- Create a Modbus TCP server with `mbus_tcp_server_new`
- Register read/write callbacks for FC01 (coils), FC03 (holding registers), FC05 (write coil)
- Run an in-process self-test that feeds synthetic Modbus TCP ADUs through the server transport
- Verify the response frames and clean up

## Build

From the workspace root, using xtask:

```bash
# Dynamic link (shared library)
cargo run -p xtask -- build-c-demo --demo c_server_demo

# Static link (libmbus_ffi.a)
cargo run -p xtask -- build-c-demo --demo c_server_demo --static
```

Or manually:

```bash
cargo build -p mbus-ffi --features c-server,full
cmake -S mbus-ffi/examples/c_server_demo -B mbus-ffi/examples/c_server_demo/build \
      -DMBUS_FFI_LINK_STATIC=OFF
cmake --build mbus-ffi/examples/c_server_demo/build
```

## Run

Using xtask (auto-builds if the binary is missing):

```bash
cargo run -p xtask -- run-c-demo --demo c_server_demo
cargo run -p xtask -- run-c-demo --demo c_server_demo --static
```

Or directly (macOS):

```bash
DYLD_LIBRARY_PATH=target/debug mbus-ffi/examples/c_server_demo/build/c_server_demo
```

Linux:

```bash
LD_LIBRARY_PATH=target/debug mbus-ffi/examples/c_server_demo/build/c_server_demo
```

Expected output:

```text
c_server_demo: success (callbacks=3)
```

## Lock Shims

`main.c` must provide implementations of the lock shims that `mbus-ffi` calls
internally.  See the "Lock Shims" section in
[`c_server_demo_yaml/README.md`](../c_server_demo_yaml/README.md) for a full
explanation and the required symbol list.
