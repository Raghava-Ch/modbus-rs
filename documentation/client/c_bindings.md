# C/FFI Client Bindings

Native C/C++ integration for Modbus client functionality.

---

## Overview

The `mbus-ffi` crate provides C-compatible bindings for use in native applications.

**Use cases:**
- Embedding in C/C++ applications
- Integration with industrial software and 3D runtimes (Unity/Unreal)
- Embedded microcontroller C projects (via custom transport integration)
- Platform-specific native apps

> Note: "game engines" here means industrial visualization/simulation use cases
> (digital twins, operator training, HMI overlays, hardware-in-the-loop dashboards),
> not gameplay protocol logic.

---

## Prerequisites

1. Build the shared library:

```bash
cargo build -p mbus-ffi --release
```

2. Find the library at:
   - macOS: `target/release/libmbus_ffi.dylib`
   - Linux: `target/release/libmbus_ffi.so`
   - Windows: `target/release/mbus_ffi.dll`

3. Copy the header file:
    - `target/mbus-ffi/include/modbus_rs_client.h`

---

## Header Generation

The header is generated using cbindgen. To regenerate:

```bash
cargo run -p xtask -- check-header
cargo run -p xtask -- gen-feature-header
```

This produces:
- `modbus_rs_client.h` — Base header
- `modbus_rs_client_feature_gated.h` — Feature-gated header

---

## C API Quick Start (Current API)

### Include Header

```c
#include "modbus_rs_client.h"
```

The native C API is ID-based (not pointer-based):

- Create a client with `mbus_tcp_client_new(...)` or `mbus_serial_client_new(...)`.
- You receive an opaque `MbusClientId`.
- Queue requests with `mbus_tcp_*` / `mbus_serial_*` functions.
- Drive state with `mbus_tcp_poll(id)` / `mbus_serial_poll(id)`.
- Handle responses via callbacks from `MbusCallbacks`.

Minimal TCP flow:

```c
#include "modbus_rs_client.h"

static void on_read_coils(const MbusReadCoilsCtx *ctx) {
    (void)ctx;
}

int main(void) {
    MbusTransportCallbacks transport = {0};
    MbusCallbacks callbacks = {0};
    callbacks.on_read_coils = on_read_coils;

    MbusTcpConfig cfg = {
        .host = "192.168.1.10",
        .port = 502,
        .response_timeout_ms = 2000,
        .retry_attempts = 1,
    };

    MbusClientId id = mbus_tcp_client_new(&cfg, &transport, &callbacks);
    if (id == MBUS_INVALID_CLIENT_ID) {
        return 1;
    }

    if (mbus_tcp_connect(id) != MbusOk) {
        mbus_tcp_client_free(id);
        return 1;
    }

    MbusStatusCode st = mbus_tcp_read_coils(id, /*txn*/1, /*unit*/1, /*addr*/0, /*qty*/16);
    if (st != MbusOk) {
        mbus_tcp_disconnect(id);
        mbus_tcp_client_free(id);
        return 1;
    }

    while (mbus_tcp_has_pending_requests(id)) {
        mbus_tcp_poll(id);
    }

    mbus_tcp_disconnect(id);
    mbus_tcp_client_free(id);
    return 0;
}
```

### Poll and Handle Responses

```c
// TCP variant
while (mbus_tcp_has_pending_requests(id)) {
    mbus_tcp_poll(id);
    usleep(10000); // 10ms
}

// Serial variant
while (mbus_serial_has_pending_requests(id)) {
    mbus_serial_poll(id);
    usleep(10000); // 10ms
}
```

This reduces unnecessary polling when there are no in-flight requests.

### Status Strings

Use `mbus_status_str(...)` to print a human-readable status:

```c
MbusStatusCode st = mbus_tcp_connect(id);
if (st != MbusOk) {
    fprintf(stderr, "connect failed: %s\n", mbus_status_str(st));
}
```

---

## Serial RTU Example

```c
MbusSerialConfig config = {
    .port_path = "/dev/ttyUSB0",
    .mode = MBUS_SERIAL_MODE_RTU,
    .baud_rate = 19200,
    .data_bits = 8,
    .stop_bits = 1,
    .parity = MBUS_PARITY_EVEN,
    .response_timeout_ms = 1000,
    .retry_attempts = 3,
};

MbusClientId id = mbus_serial_client_new(&config, &transport, &callbacks);
```

---

## CMake Integration

### CMakeLists.txt

```cmake
cmake_minimum_required(VERSION 3.16)
project(my_modbus_app)

# Find the library
find_library(MBUS_FFI_LIB mbus_ffi
    PATHS ${CMAKE_SOURCE_DIR}/../target/release
)

# Include header
include_directories(${CMAKE_SOURCE_DIR}/../target/mbus-ffi/include)

add_executable(my_app main.c)
target_link_libraries(my_app ${MBUS_FFI_LIB})
```

### Build

```bash
mkdir build && cd build
cmake ..
make
```

---

## Error Handling

Native C functions return `MbusStatusCode`.

- `MbusOk` means the request was accepted/queued.
- Response or protocol failures are delivered later via callbacks.
- Use `mbus_status_str(code)` for text output.

---

## Thread Safety

- A single `MbusClientId` is not re-entrant from callbacks.
- Define required lock hooks (`mbus_pool_lock/unlock`, `mbus_client_lock/unlock`) in your C app.
- Callback functions are called from the thread that calls `mbus_tcp_poll()` / `mbus_serial_poll()`

---

## Memory Management

| Function | Allocates | Free With |
|----------|-----------|-----------|
| `mbus_tcp_client_new` | Yes | `mbus_tcp_client_free` |
| `mbus_serial_client_new` | Yes | `mbus_serial_client_free` |
| Response buffers | Internal | Automatically managed |

---

## Callback API

Requests are asynchronous: queue functions return quickly, and results are delivered
through function pointers in `MbusCallbacks`.

The callback signatures use context structs (for example `MbusReadCoilsCtx`),
and failure callbacks report `MbusStatusCode`.

For exact callback field names and signatures, refer to:

- `target/mbus-ffi/include/modbus_rs_client.h`
- [mbus-ffi/examples/c_client_demo/main.c](../../mbus-ffi/examples/c_client_demo/main.c)

Skeleton:

```c
MbusCallbacks callbacks = {0};
/* assign callbacks.on_read_coils, callbacks.on_request_failed, ... */

MbusClientId id = mbus_tcp_client_new(&cfg, &transport, &callbacks);
```

---

## C Smoke Test

A pre-built C example is available:

```bash
# Build
cargo run -p xtask -- build-c-demo --demo c_client_demo

# Run (serial PTY loopback)
cargo run -p xtask -- run-c-demo --demo c_client_demo --mode serial-pty
```

Source: [mbus-ffi/examples/c_client_demo/main.c](../../mbus-ffi/examples/c_client_demo/main.c)

---

## See Also

- [WASM Development](wasm.md) — Browser WebSocket client
- [Feature Flags](feature_flags.md) — FFI build options
- [Sync Development](sync.md) — Rust sync client guide
