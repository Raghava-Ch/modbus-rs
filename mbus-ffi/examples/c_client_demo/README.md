# c_client_demo

C FFI smoke test for the `mbus-ffi` **client** bindings.  Tests both TCP and Serial
transports using an in-process PTY loopback so no real hardware or network is required.

What it demonstrates:
- Create a Modbus TCP client connection with `mbus_client_new_tcp`
- Create a Modbus Serial client connection with `mbus_client_new_serial` over a PTY pair
- Send read-coils (FC01), read-holding-registers (FC03), and write-coil (FC05) requests
- Parse and verify Modbus response PDUs
- Clean up client handles

## Build

From the workspace root, using xtask:

```bash
# Dynamic link
cargo run -p xtask -- build-c-demo --demo c_client_demo

# Static link
cargo run -p xtask -- build-c-demo --demo c_client_demo --static
```

Or manually:

```bash
cargo build -p mbus-ffi --features c,full
cmake -S mbus-ffi/examples/c_client_demo -B mbus-ffi/examples/c_client_demo/build \
      -DMBUS_FFI_LINK_STATIC=OFF
cmake --build mbus-ffi/examples/c_client_demo/build
```

## Run

Using xtask (auto-builds if the binary is missing):

```bash
# PTY loopback mode (default, no hardware needed)
cargo run -p xtask -- run-c-demo --demo c_client_demo
cargo run -p xtask -- run-c-demo --demo c_client_demo --mode serial-pty
```

Or directly (macOS):

```bash
DYLD_LIBRARY_PATH=target/debug mbus-ffi/examples/c_client_demo/build/c_smoke_test --serial-pty
```

Linux:

```bash
LD_LIBRARY_PATH=target/debug mbus-ffi/examples/c_client_demo/build/c_smoke_test --serial-pty
```

## Lock Shims

`main.c` provides `mbus_pool_lock` / `mbus_pool_unlock` and per-client
`mbus_client_lock` / `mbus_client_unlock`.  These are called internally by the Rust
library via RAII; **never call them from application code** (deadlock).
