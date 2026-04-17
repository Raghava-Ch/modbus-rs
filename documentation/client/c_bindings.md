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
   - [mbus-ffi/include/mbus_ffi.h](../../mbus-ffi/include/mbus_ffi.h)

---

## Header Generation

The header is generated using cbindgen. To regenerate:

```bash
cargo run -p xtask -- check-header
cargo run -p xtask -- gen-feature-header
```

This produces:
- `mbus_ffi.h` — Base header
- `mbus_ffi_feature_gated.h` — Full header with all features

---

## C API Quick Start

### Include Header

```c
#include "mbus_ffi.h"
```

### Create Client

```c
MbusClient* client = mbus_client_new_tcp("192.168.1.10", 502, NULL);
if (!client) {
    fprintf(stderr, "Failed to create client\n");
    return 1;
}
```

### Connect

```c
MbusError err = mbus_client_connect(client);
if (err != MBUS_ERROR_NONE) {
    fprintf(stderr, "Connect failed: %d\n", err);
    mbus_client_free(client);
    return 1;
}
```

### Read Coils

```c
uint16_t txn_id;
err = mbus_client_read_coils(client, 1, 0, 16, &txn_id);
if (err != MBUS_ERROR_NONE) {
    fprintf(stderr, "Read coils failed: %d\n", err);
}
```

### Poll and Handle Response

```c
// TCP variant
while (mbus_tcp_has_pending_requests(client_id)) {
    mbus_tcp_poll(client_id);
    usleep(10000); // 10ms
}

// Serial variant
while (mbus_serial_has_pending_requests(client_id)) {
    mbus_serial_poll(client_id);
    usleep(10000); // 10ms
}
```

This reduces unnecessary polling when there are no in-flight requests.

### Cleanup

```c
mbus_client_disconnect(client);
mbus_client_free(client);
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

MbusClient* client = mbus_client_new_serial(&config, NULL);
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
include_directories(${CMAKE_SOURCE_DIR}/../mbus-ffi/include)

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

All functions return `MbusError` enum:

```c
typedef enum {
    MBUS_ERROR_NONE = 0,
    MBUS_ERROR_TIMEOUT,
    MBUS_ERROR_CONNECTION_LOST,
    MBUS_ERROR_INVALID_RESPONSE,
    MBUS_ERROR_EXCEPTION_RESPONSE,
    // ...
} MbusError;
```

Use `mbus_error_to_string()` for human-readable messages:

```c
MbusError err = mbus_client_connect(client);
if (err != MBUS_ERROR_NONE) {
    fprintf(stderr, "Error: %s\n", mbus_error_to_string(err));
}
```

---

## Thread Safety

- A single `MbusClient*` is **NOT** thread-safe
- Create separate clients per thread, or use external synchronization
- Callback functions are called from the thread that calls `mbus_tcp_poll()` / `mbus_serial_poll()`

---

## Memory Management

| Function | Allocates | Free With |
|----------|-----------|-----------|
| `mbus_client_new_tcp` | Yes | `mbus_client_free` |
| `mbus_client_new_serial` | Yes | `mbus_client_free` |
| Response buffers | Internal | Automatically managed |

---

## Callback API

For asynchronous notification:

```c
void on_coils_response(uint16_t txn_id, uint8_t unit_id, const uint8_t* values, uint16_t quantity) {
    printf("Coils response: txn=%d, qty=%d\n", txn_id, quantity);
}

void on_error(uint16_t txn_id, uint8_t unit_id, MbusError error) {
    fprintf(stderr, "Request %d failed: %s\n", txn_id, mbus_error_to_string(error));
}

MbusCallbacks callbacks = {
    .on_coils_response = on_coils_response,
    .on_error = on_error,
};

MbusClient* client = mbus_client_new_tcp("192.168.1.10", 502, &callbacks);
```

---

## C Smoke Test

A pre-built C example is available:

```bash
# Build
cargo run -p xtask -- build-c-smoke

# Run (serial PTY loopback)
./mbus-ffi/examples/c_smoke_cmake/build/c_smoke_test --serial-pty
```

Source: [mbus-ffi/examples/c_smoke_cmake/main.c](../../mbus-ffi/examples/c_smoke_cmake/main.c)

---

## See Also

- [WASM Development](wasm.md) — Browser WebSocket client
- [Feature Flags](feature_flags.md) — FFI build options
- [Building Applications](building_applications.md) — Rust client guide
