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
To increase maximum clients, inject the environment variable safely:
```bash
MBUS_MAX_CLIENTS=10 cargo build -p mbus-ffi --features c,full
```

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

*For a full operational POSIX socket example, view the `mbus-ffi/examples/c_smoke_cmake/main.c` build schema!*

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
	"ws://127.0.0.1:8080", // ws_url
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

## License
Licensed under the repository root `LICENSE`.
