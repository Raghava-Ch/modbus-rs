# mbus-ffi

WASM/JS and Native C/C++ FFI bindings for the `modbus-rs` stack.

## Position In Workspace

`mbus-ffi` is an implementation wrapper crate inside this workspace. It encapsulates the core state machines of the `modbus-rs` stack, mapping them natively across two distinct abstraction boundaries:
1. **Web Run-times (WASM)** via Javascript Promises.
2. **Native Run-times (C/C++)** via opaque pointers, static client ID pools, and dependency-injected function callbacks.

---

## Native C/C++ Bindings (FFI)

The native FFI is designed specifically for **Strict `no_std`** and embedded use cases:
- **Zero Heap Allocations**: The FFI path absolutely avoids `alloc` (No `Box`, `Vec`, or dynamic dispatch).
- **Static Client Pool**: All Modbus clients allocated for native FFI exist in a thread-safe static pool using an ID-based system (`MbusClientId`), preventing raw pointers from leaking across boundaries to easily integrate with memory-unsafe languages.
- **Zero OS dependencies**: TCP/Serial abstractions are stripped out native compilation. The host application performs all network sockets, timer integrations, and byte transmissions by sending them upward into the Modbus stack via `MbusTransportCallbacks`.
- **`panic=abort`**: Automatically detects `no_std` execution properties gracefully through `build.rs`.

### Pool Configuration
Pool sizing is determined strictly at compile time. By default, the system provisions exactly `1` slot.
To increase maximum clients, set both environment variables explicitly:
```bash
MBUS_MAX_TCP_CLIENTS=10 MBUS_MAX_SERIAL_CLIENTS=10 cargo build -p mbus-ffi --features c,full
```

- `MBUS_MAX_TCP_CLIENTS`: valid range `1..=127`
- `MBUS_MAX_SERIAL_CLIENTS`: valid range `1..=126`

### Build & Link
`mbus-ffi` supports compiling directly to shared (`.so`/`.dylib`) and static (`.a`) libraries:
```bash
cargo build --release -p mbus-ffi --features c,full
```

*Note: Even in strict `no_std` environments, standard LLVM targets like `target/debug` on mac/linux will naturally map underlying memory routines (`memcpy`, `memmove`) strictly via system libc. When targeting explicit embedded system triples like `thumbv7em-none-eabihf`, compiler built-ins will resolve them.*

### Automatic `mbus_ffi.h` Header Generation
We utilize `cbindgen` to define memory-perfect opaque wrappers for external model parsing:
```bash
cbindgen --config mbus-ffi/cbindgen.toml --crate mbus-ffi --output mbus-ffi/include/mbus_ffi.h
```

### Header / Feature Compatibility
`mbus_ffi.h` is generated from the Rust API shape of the enabled feature set.
The checked-in header is intended for native C builds with:

```bash
--features c,full
```

### C API Quick Start (Transport Polling)

Instead of passing system sockets, you attach your exact runtime logic using POSIX or embedded UART controls directly via `MbusTransportCallbacks`:

```c
#include "mbus_ffi.h"

// 1. Setup specific connection rules
struct MbusTcpConfig config = {0};
config.host = "192.168.1.10";
config.port = 502;
// ... (timeouts/retries)

// 2. Setup your OS networking functions
struct MbusTransportCallbacks transport = {0};
transport.userdata = &my_posix_socket_context;
transport.on_connect = my_os_connect;
transport.on_send = my_os_send;
transport.on_recv = my_os_recv;
// ... 

// 3. Setup Response callbacks
struct MbusCallbacks app_callbacks = {0};
app_callbacks.on_read_coils = my_app_read_coils;
// ...

MbusClientId client_id = mbus_tcp_client_new(&config, &transport, &app_callbacks);

// Request the connection internally
mbus_tcp_connect(client_id);
mbus_tcp_read_coils(client_id, 42 /* txn_id */, 1 /* unit_id */, 0 /* address */, 10 /* quantity */);

// Must be continuously ticked within your device's task loop
while(1) {
    mbus_tcp_poll(client_id);
}
```

*For a full operational POSIX socket example and a self-contained serial PTY smoke path, view `mbus-ffi/examples/c_smoke_cmake/main.c`.*

### C Smoke Example
Build the smoke example with xtask:

```bash
cargo run -p xtask -- build-c-smoke
```

This configures and builds the CMake target, then runs a PTY-backed serial RTU smoke test via CTest.
The smoke binary also supports manual execution:

```bash
cd mbus-ffi/examples/c_smoke_cmake/build
./c_smoke_test --serial-pty
./c_smoke_test --tcp 127.0.0.1 502
```

The `--serial-pty` mode is self-contained and does not require hardware. It creates a pseudo-terminal pair,
opens the slave side through the FFI serial client, and serves a single RTU read-coils response from the
master side so CI can exercise the serial path deterministically.

### Running FFI Tests

The FFI layer has three distinct test paths:

1. Rust-side unit tests inside `mbus-ffi`
2. Native C binding-layer tests compiled from `mbus-ffi/tests/c_api/test_binding_layer.c`
3. Higher-level native smoke tests under `examples/c_smoke_cmake`

#### Rust-side FFI tests

Run the Rust unit tests and doc tests for `mbus-ffi`:

```bash
cargo test -p mbus-ffi
```

This validates the Rust-side FFI mapping code such as config translation, error/status mapping,
and static client-pool behavior.

#### Native C binding-layer tests

First build the native FFI library with the C API enabled:

```bash
cargo build -p mbus-ffi --features c,full
```

Then compile the standalone C binding test source:

```bash
cc -I mbus-ffi/include mbus-ffi/tests/c_api/test_binding_layer.c -L target/debug -lmbus_ffi -o /tmp/test_binding_layer
```

Run it against the freshly built shared library:

macOS:

```bash
DYLD_LIBRARY_PATH="$PWD/target/debug" /tmp/test_binding_layer
```

Linux:

```bash
LD_LIBRARY_PATH="$PWD/target/debug" /tmp/test_binding_layer
```

This exercises the C ABI directly from C code, covering null-handling, invalid client IDs,
status strings, accessor helpers, and native error paths.

If you already have the CMake-based test harness built under `mbus-ffi/tests/c_api/build`, you can also run:

```bash
./mbus-ffi/tests/c_api/build/c_api_tests
```

#### Native C smoke test

Run the end-to-end smoke path with xtask:

```bash
cargo run -p xtask -- build-c-smoke
```

This builds `mbus-ffi` with `--features c,full`, configures the CMake smoke project,
builds the `c_smoke_test` in ./mbus-ffi/examples/c_smoke_cmake/main.c, and runs the registered CTest cases.

Use this path for a higher-level native integration check in addition to the direct C binding-layer tests above.

---

## Thread Safety

The C FFI layer is designed to be thread-safe, but it requires the **host application** to provide
four lock/unlock symbols that the Rust library resolves at **link time**:

```c
// Declared in mbus_ffi.h — you must define all four in your C/C++ project.
void mbus_pool_lock(void);
void mbus_pool_unlock(void);
void mbus_client_lock(MbusClientId id);
void mbus_client_unlock(MbusClientId id);
```

These are **not** function pointers in `MbusTransportCallbacks` — they are ordinary `extern` symbols
that the linker expects to find in your object files, exactly like implementing `malloc` for a
bare-metal libc. The header declares them; your application defines them.

| Symbol | Called when |
|---|---|
| `mbus_pool_lock` / `mbus_pool_unlock` | Any operation that allocates or frees a client slot |
| `mbus_client_lock(id)` / `mbus_client_unlock(id)` | Any operation that reads or mutates a specific client |

**Single-threaded** — define them as no-ops:
```c
void mbus_pool_lock(void)  {}
void mbus_pool_unlock(void) {}
void mbus_client_lock(MbusClientId id)   { (void)id; }
void mbus_client_unlock(MbusClientId id) { (void)id; }
```

**Multi-threaded** — back them with real mutexes:
```c
static pthread_mutex_t g_pool_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t g_client_mutex[256] = { [0 ... 255] = PTHREAD_MUTEX_INITIALIZER };

void mbus_pool_lock(void)   { pthread_mutex_lock(&g_pool_mutex); }
void mbus_pool_unlock(void) { pthread_mutex_unlock(&g_pool_mutex); }
void mbus_client_lock(MbusClientId id)   { pthread_mutex_lock(&g_client_mutex[id]); }
void mbus_client_unlock(MbusClientId id) { pthread_mutex_unlock(&g_client_mutex[id]); }
```

### Callbacks fire inside the client lock

**All response callbacks** (`on_read_coils`, `on_request_failed`, …) are invoked
**synchronously from within `mbus_tcp_poll` / `mbus_serial_poll`**, while
`mbus_client_lock(id)` is already held for that client.

Consequence: **do not call any `mbus_tcp_*` / `mbus_serial_*` API from inside a
callback** — the attempt to re-acquire the same client lock will deadlock or return
`MBUS_ERR_BUSY`. Queue follow-up requests in a user-owned buffer and enqueue
them after `poll` returns.

---

## WASM Browser Bindings

`mbus-ffi` securely exports internal modbus logic to JavaScript via `wasm-pack`, exposing:
- `WasmModbusClient` (WebSocket transport mapper)
- `WasmSerialModbusClient` + `request_serial_port()` (Web Serial hardware mapper)

All APIs are Promise-based and are designed specifically for browser runtimes (`wasm32`). Building native targets does not interact with Javascript wrappers.

### Build WASM Package
```bash
wasm-pack build --target web --features wasm,full
```
Generated JS/WASM package is written to `mbus-ffi/pkg`.

### Quick Start (WebSocket)
```javascript
import init, { WasmModbusClient } from "./pkg/mbus_ffi.js";

await init();

const client = new WasmModbusClient(
	"ws://127.0.0.1:8080", // ws_proxy_url
	1,                      // unit_id
	3000,                   // response_timeout_ms
	1,                      // retry_attempts
	20                      // tick_interval_ms
);

const regs = await client.read_holding_registers(0, 2);
console.log(Array.from(regs));
```

### Quick Start (Web Serial)
```javascript
import init, { request_serial_port, WasmSerialModbusClient } from "./pkg/mbus_ffi.js";

await init();

// Must be called from a user gesture (e.g. button double click)
const portHandle = await request_serial_port();

const client = new WasmSerialModbusClient(
	portHandle,
	1,      // unit_id
	"rtu",  // mode: "rtu" | "ascii"
	9600,   // baud_rate
	... 
);
const ok = await client.read_single_coil(0);
```

### Example Web Pages
Use the browser examples under `mbus-ffi/examples`:
- `network_smoke.html` (WebSocket/TCP path)
- `serial_smoke.html` (Web Serial path, full serial API smoke runner)

Serve the examples over localhost:
```bash
cd mbus-ffi
python3 -m http.server 8089
```

## Supported Modbus Operations
Both FFI wrappers expose the same internal client services configured by feature flags:
- `coils`: read single/multiple, write single/multiple
- `registers`: read holding/input, write single/multiple, mask write, read-write multiple
- `discrete-inputs`: read single/multiple
- `fifo`: read
- `file-record`: read/write
- `diagnostics`: exception status, diagnostics, comm event counter/log, report server id, read device ID
- `full`: Enables all Modbus service features.

## Licensing

Copyright (C) 2025 Raghava Challari

This project is currently licensed under the GNU General Public License v3.0 (GPLv3) for evaluation purposes.

For details, refer to the [LICENSE](./LICENSE) file or the [GPLv3 official site](https://www.gnu.org/licenses/gpl-3.0.en.html).

## Contact

**Name:** Raghava Ch  
**Email:** [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)  
**Repository:** [github.com/Raghava-Ch/modbus-rs](https://github.com/Raghava-Ch/modbus-rs)
