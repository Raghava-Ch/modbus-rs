# mbus-ffi

WASM/JS, Native C/C++ FFI, **Python**, and **.NET (C#)** bindings for the `modbus-rs` stack.

---

## Python Bindings (`modbus-rs` on PyPI)

The `python` feature compiles `mbus-ffi` into a Python extension module via
[PyO3](https://pyo3.rs) and [Maturin](https://maturin.rs).
PyPI package: **`modbus-rs`** · Import name: `modbus_rs`

### Installation

```bash
pip install modbus-rs
```

### Quick Start — Synchronous TCP client

```python
import modbus_rs

with modbus_rs.TcpClient("192.168.1.10", port=502, unit_id=1) as client:
    client.connect()
    regs = client.read_holding_registers(0, 10)
    print(regs)                     # [0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    client.write_register(0, 0xFF)
    coils = client.read_coils(0, 8)
    print(coils)                    # [False, False, ...]
```

### Quick Start — Asyncio TCP client

```python
import asyncio
import modbus_rs

async def main():
    async with modbus_rs.AsyncTcpClient("192.168.1.10", unit_id=1) as client:
        regs = await client.read_holding_registers(0, 10)
        print(regs)

asyncio.run(main())
```

### Quick Start — Serial client (RTU)

```python
import modbus_rs

with modbus_rs.SerialClient("/dev/ttyUSB0", baud_rate=9600, unit_id=1) as client:
    client.connect()
    regs = client.read_holding_registers(0, 5)
    print(regs)
```

### Quick Start — TCP server

```python
import asyncio
import modbus_rs

class MyApp(modbus_rs.ModbusApp):
    def handle_read_holding_registers(self, address, count):
        return [address + i for i in range(count)]

    def handle_write_register(self, address, value):
        pass  # accept silently

async def main():
    async with modbus_rs.AsyncTcpServer("0.0.0.0", MyApp(), port=5020, unit_id=1) as srv:
        await srv.serve_forever()

asyncio.run(main())
```

### Exception hierarchy

```
ModbusError
├── ModbusTimeout
├── ModbusConnectionError
├── ModbusProtocolError
│   └── ModbusDeviceException
└── ModbusConfigError
```

### Building from source

Requires a Rust toolchain and [Maturin](https://maturin.rs) (`pip install maturin`).

```bash
# install into an active venv (editable / dev mode)
cd mbus-ffi && maturin develop --features python,full

# build a release wheel
cd mbus-ffi && maturin build --release --features python,full
```

### Running the Python test suite

```bash
pip install pytest pytest-asyncio
pytest mbus-ffi/tests/python/ -v
```

### Modbus TCP Gateway (`python-gateway` feature)

Enable the optional `python-gateway` feature to expose `TcpGateway` (sync) and
`AsyncTcpGateway` (asyncio) classes that bridge an upstream Modbus TCP listener
to one or more downstream Modbus TCP servers.

```bash
cd mbus-ffi && maturin develop --features python,python-gateway,full
```

Minimal sync example:

```python
import modbus_rs

gw = modbus_rs.TcpGateway("0.0.0.0:5020")
ch = gw.add_tcp_downstream("192.168.1.10", 502)
gw.add_unit_route(unit=1, channel=ch)
gw.serve_forever()  # call gw.stop() from another thread to exit
```

Runnable demos: [`examples/python_gateway/sync_demo.py`](examples/python_gateway/sync_demo.py)
and [`examples/python_gateway/async_demo.py`](examples/python_gateway/async_demo.py).

> The `event_handler=GatewayEventHandler()` constructor argument is currently
> accepted but not invoked — the underlying async gateway server has no event
> hook surface yet. The class exists for forward compatibility.

---

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

- `MBUS_MAX_TCP_CLIENTS`: valid range `1..=255`
- `MBUS_MAX_SERIAL_CLIENTS`: valid range `1..=255`

### Build & Link
`mbus-ffi` supports compiling directly to shared (`.so`/`.dylib`) and static (`.a`) libraries:
```bash
cargo build --release -p mbus-ffi --features c,full
```

*Note: Even in strict `no_std` environments, standard LLVM targets like `target/debug` on mac/linux will naturally map underlying memory routines (`memcpy`, `memmove`) strictly via system libc. When targeting explicit embedded system triples like `thumbv7em-none-eabihf`, compiler built-ins will resolve them.*

### Automatic C Header Generation
We use `cbindgen` to emit the public C headers from the current Rust API surface.

Client header:
```bash
cbindgen --config mbus-ffi/cbindgen_client.toml --crate mbus-ffi --output target/mbus-ffi/include/modbus_rs_client.h
```

Server header:

```bash
cbindgen --config mbus-ffi/cbindgen_server.toml --crate mbus-ffi --output target/mbus-ffi/include/modbus_rs_server.h
```

The workspace also provides xtask helpers for the client headers:

```bash
cargo run -p xtask -- gen-header
cargo run -p xtask -- gen-feature-header
```

### Header / Feature Compatibility
`modbus_rs_client.h` is generated from the Rust API shape of the enabled feature set.
The generated header is intended for native C builds with:

```bash
--features c,full
```

For native server builds, `modbus_rs_server.h` is generated from the `c-server`
API surface, while YAML-driven server apps additionally emit
`target/mbus-ffi/include/mbus_server_app.h` from the device config.

### C API Quick Start (Transport Polling)

Instead of passing system sockets, you attach your exact runtime logic using POSIX or embedded UART controls directly via `MbusTransportCallbacks`:

```c
#include "modbus_rs_client.h"

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

*For a full operational POSIX socket example and a self-contained serial PTY smoke path, view `mbus-ffi/examples/c_client_demo/main.c`.*

### C Smoke Example
Build the client smoke demo with xtask:

```bash
cargo run -p xtask -- build-c-demo --demo c_client_demo
```

This configures and builds the CMake target for `mbus-ffi/examples/c_client_demo`,
then runs its registered CTest cases.
The smoke binary also supports manual execution:

```bash
cd mbus-ffi/examples/c_client_demo/build
./c_smoke_test --serial-pty
./c_smoke_test --tcp 127.0.0.1 502
```

The `--serial-pty` mode is self-contained and does not require hardware. It creates a pseudo-terminal pair,
opens the slave side through the FFI serial client, and serves a single RTU read-coils response from the
master side so CI can exercise the serial path deterministically.

### Modbus TCP Gateway (`c-gateway` feature)

The `c-gateway` feature exposes a `no_std` C API that mirrors the
client/server pool design: the application provides upstream and downstream
transport callbacks (`MbusTransportCallbacks`) and `mbus_pool_lock` /
`mbus_pool_unlock` plus `mbus_gateway_lock(id)` / `mbus_gateway_unlock(id)`
routines, then drives the gateway with `mbus_gateway_poll(id)` from its event
loop.

```bash
cargo build -p mbus-ffi --features c-gateway --no-default-features
```

A complete end-to-end CMake demo (POSIX sockets, in-process echo Modbus server,
event logging) lives at [`examples/c_gateway_demo/main.c`](examples/c_gateway_demo/main.c).
Build and run it with:

```bash
cd mbus-ffi/examples/c_gateway_demo
mkdir build && cd build
CC=/usr/bin/clang cmake ..   # macOS: avoid Homebrew llvm
make
./c_gateway_demo
```

### Running FFI Tests

The FFI layer has three distinct test paths:

1. Rust-side unit tests inside `mbus-ffi`
2. Native C binding-layer tests compiled from `mbus-ffi/tests/c_api/test_binding_layer.c`
3. Higher-level native smoke tests under `examples/c_client_demo`

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
cc -I target/mbus-ffi/include mbus-ffi/tests/c_api/test_binding_layer.c -L target/debug -lmbus_ffi -o /tmp/test_binding_layer
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

Build or run the end-to-end client smoke demo with xtask:

```bash
cargo run -p xtask -- build-c-demo --demo c_client_demo
cargo run -p xtask -- run-c-demo --demo c_client_demo --mode serial-pty
```

This builds `mbus-ffi` with the demo's declared feature set, configures the CMake
project in `mbus-ffi/examples/c_client_demo`, builds `c_smoke_test`, and can run
the PTY-backed smoke path without external hardware.

### C Server — Two Integration Approaches

There are two ways to build a Modbus TCP server with the C FFI.

#### Approach 1: Hand-Written Handlers (`c_server_demo`)

All FC callbacks are wired directly in `main.c` without any code generation.
This is the simplest starting point when the register layout is small or fixed.

```bash
# Build and self-test
cargo run -p xtask -- build-c-demo --demo c_server_demo

# Static link
cargo run -p xtask -- build-c-demo --demo c_server_demo --static
```

Source: `mbus-ffi/examples/c_server_demo/main.c`.
It creates a Modbus TCP server via `mbus_tcp_server_new`, registers FC01/FC05/FC03
callbacks, runs an in-process self-test with synthetic ADUs, and validates responses.

#### Approach 2: YAML-Driven Code Generation (`c_server_demo_yaml`)

The device register/coil map is declared in a YAML file.  `build.rs` generates a
type-safe Rust dispatcher into `$OUT_DIR` at compile time (never tracked in git),
and `gen-server-app` regenerates the matching C header.

```bash
# Build, generate C header, compile, and self-test
MBUS_SERVER_APP_CONFIG=mbus-ffi/examples/c_server_demo_yaml/mbus_server_app.example.yaml \
	cargo run -p xtask -- build-c-demo --demo c_server_demo_yaml --static --features c-server
```

Or through xtask (sets the env var automatically from `demo.yaml`):

```bash
cargo run -p xtask -- build-c-demo --demo c_server_demo_yaml --static
```

Source: `mbus-ffi/examples/c_server_demo_yaml/`.
Full walkthrough: [`examples/c_server_demo_yaml/README.md`](examples/c_server_demo_yaml/README.md).

When `MBUS_SERVER_APP_CONFIG` is not set, `build.rs` falls back to the bundled
`examples/c_server_demo_yaml/mbus_server_app.example.yaml` when building inside
this workspace. That fallback exists to keep workspace builds and tests working;
external `mbus-ffi` consumers should set `MBUS_SERVER_APP_CONFIG` explicitly.

#### Choosing an approach

| | Hand-written | YAML-driven |
|---|---|---|
| Register map changes | Edit C callbacks directly | Edit YAML → rebuild auto-updates C header and Rust dispatcher |
| Generated files in git | None | None |
| Rust dispatcher source | Written by hand | Generated by `build.rs` into `$OUT_DIR` |
| Write-notification hooks | Your C function, wired manually | Declared in YAML, called by generated dispatcher |
| Best for | Quick integration / small maps | Production devices with many registers |

---

## .NET (C#) Bindings

The `dotnet` feature compiles the `mbus_dn_*` entry-point family — a flat,
versioned C ABI specifically designed for .NET P/Invoke (`[LibraryImport]`).
The managed wrapper lives in [`mbus-ffi/dotnet/`](dotnet/).

```bash
# Build the native cdylib with .NET support and all FCs
cargo build -p mbus-ffi --features dotnet,full
```

Output: `target/debug/mbus_ffi.dll` (Windows) / `libmbus_ffi.so` (Linux) /
`libmbus_ffi.dylib` (macOS).

### Native DLL search

`[LibraryImport("mbus_ffi")]` loads the native library from the application
output directory, `PATH`, or the working directory.  The example projects copy
the DLL from `target\debug\` (or `target\release\`) automatically via MSBuild.
See [DLL Deployment](../documentation/dotnet_bindings.md#native-dll-deployment)
for the full copy rule and VS 2022 setup.

### Quick start (C#)

```csharp
using ModbusRs;

using var client = new ModbusTcpClient("192.168.1.10", 502);
await client.ConnectAsync();

ushort[] regs = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);
bool[] coils  = await client.ReadCoilsAsync(unitId: 1, address: 0, quantity: 8);

await client.DisconnectAsync();
```

📖 **[Full .NET Binding Documentation →](../documentation/dotnet_bindings.md)**

---

## Thread Safety

The C FFI layer is designed to be thread-safe, but it requires the **host application** to provide
lock/unlock symbols that the Rust library resolves at **link time**.

For the client FFI, define:

```c
// Required by the native client pool and client operations.
void mbus_pool_lock(void);
void mbus_pool_unlock(void);
void mbus_client_lock(MbusClientId id);
void mbus_client_unlock(MbusClientId id);
```

For the server FFI, define:

```c
// Required by the native server pool and server operations.
void mbus_pool_lock(void);
void mbus_pool_unlock(void);
void mbus_server_lock(MbusServerId id);
void mbus_server_unlock(MbusServerId id);
```

These are **not** function pointers in `MbusTransportCallbacks` — they are ordinary `extern` symbols
that the linker expects to find in your object files, exactly like implementing `malloc` for a
bare-metal libc. Your application defines them and the Rust FFI resolves them at link time.

| Symbol | Called when |
|---|---|
| `mbus_pool_lock` / `mbus_pool_unlock` | Any operation that allocates or frees a client or server slot |
| `mbus_client_lock(id)` / `mbus_client_unlock(id)` | Any operation that reads or mutates a specific client |
| `mbus_server_lock(id)` / `mbus_server_unlock(id)` | Any operation that polls, connects, disconnects, or mutates a specific server |

**Single-threaded** — define them as no-ops:
```c
void mbus_pool_lock(void)  {}
void mbus_pool_unlock(void) {}
void mbus_client_lock(MbusClientId id)   { (void)id; }
void mbus_client_unlock(MbusClientId id) { (void)id; }
void mbus_server_lock(MbusServerId id)   { (void)id; }
void mbus_server_unlock(MbusServerId id) { (void)id; }
```

**Multi-threaded** — back them with real mutexes:
```c
static pthread_mutex_t g_pool_mutex = PTHREAD_MUTEX_INITIALIZER;
static pthread_mutex_t g_client_mutex[256] = { [0 ... 255] = PTHREAD_MUTEX_INITIALIZER };
static pthread_mutex_t g_server_mutex[256] = { [0 ... 255] = PTHREAD_MUTEX_INITIALIZER };

void mbus_pool_lock(void)   { pthread_mutex_lock(&g_pool_mutex); }
void mbus_pool_unlock(void) { pthread_mutex_unlock(&g_pool_mutex); }
void mbus_client_lock(MbusClientId id)   { pthread_mutex_lock(&g_client_mutex[id]); }
void mbus_client_unlock(MbusClientId id) { pthread_mutex_unlock(&g_client_mutex[id]); }
void mbus_server_lock(MbusServerId id)   { pthread_mutex_lock(&g_server_mutex[id & 0xFFu]); }
void mbus_server_unlock(MbusServerId id) { pthread_mutex_unlock(&g_server_mutex[id & 0xFFu]); }
```

### Callbacks fire inside the client lock

**All response callbacks** (`on_read_coils`, `on_request_failed`, …) are invoked
**synchronously from within `mbus_tcp_poll` / `mbus_serial_poll`**, while
`mbus_client_lock(id)` is already held for that client.

Consequence: **do not call any `mbus_tcp_*` / `mbus_serial_*` API from inside a
callback** — the attempt to re-acquire the same client lock will deadlock or return
`MBUS_ERR_BUSY`. Queue follow-up requests in a user-owned buffer and enqueue
them after `poll` returns.

For the server FFI, hold `mbus_server_lock(id)` around `mbus_tcp_server_poll`,
`mbus_tcp_server_connect`, `mbus_tcp_server_disconnect`, and the corresponding
serial server calls. Use `mbus_pool_lock` / `mbus_pool_unlock` for server
allocation and free paths.

---

## WASM Browser Bindings

`mbus-ffi` securely exports internal modbus logic to JavaScript via `wasm-pack`, exposing:
- `WasmModbusClient` (WebSocket transport mapper)
- `WasmSerialModbusClient` + `request_serial_port()` (Web Serial hardware mapper)
- `WasmTcpServer` + `WasmTcpGatewayConfig` (WASM server binding over network transport)
- `WasmSerialServer` + `WasmSerialServerConfig` (WASM server binding over serial transport)

All APIs are Promise-based and are designed specifically for browser runtimes (`wasm32`). Building native targets does not interact with Javascript wrappers.

### WASM Server Transport Ownership Boundary

WASM transport implementations are owned by transport crates only:
- `mbus-network` owns the WASM websocket transport implementation.
- `mbus-serial` owns the WASM Web Serial transport implementation.

`mbus-ffi` server bindings orchestrate lifecycle and JS callback bridging; they do not reimplement transport I/O.

### WASM Server Protocol Contract (Current vs Planned)

Current contractual surface (available now):

- `WasmTcpServer` / `WasmSerialServer` lifecycle: `start()`, `stop()`, `is_running()`
- JS callback bridge via `dispatch_request(...)` (direct value or Promise return)
- Raw frame passthrough helpers: `send_frame(...)`, `recv_frame(...)`
- WebSocket readiness helpers (TCP server): `transport_connecting()`, `transport_connected()`
- Transport ownership remains in `mbus-network` and `mbus-serial`

Not part of the current server binding contract (planned later):

- Built-in Modbus function-code dispatcher/routing inside `WasmTcpServer` / `WasmSerialServer`
- Guaranteed typed FC response helpers at the WASM server binding layer
- Fully managed end-to-end server request loop owned by `mbus-ffi`

This separation is intentional for phase stability: current bindings provide lifecycle + bridge + transport plumbing, while higher-level protocol handling is integrated incrementally.

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

### Quick Start (WASM Server Bindings)

```javascript
import init, {
	WasmTcpServer,
	WasmTcpGatewayConfig,
	WasmSerialServer,
	WasmSerialServerConfig,
} from "./pkg/mbus_ffi.js";

await init();

const tcpServer = new WasmTcpServer(
	new WasmTcpGatewayConfig("ws://127.0.0.1:8080"),
	(req) => Promise.resolve({ ok: true, echo: req })
);
await tcpServer.start();

const serialServer = new WasmSerialServer(
	WasmSerialServerConfig.rtu(),
	(req) => Promise.resolve(req)
);
// Attach browser SerialPort object from navigator.serial.requestPort() before start.
```

### Server Observability (Status + Last Error)

Use `status_snapshot()` for lightweight counters and lifecycle state, and
`last_error_message()` / `clear_last_error()` to track and reset the latest
binding-level failure.

```javascript
const snap = tcpServer.status_snapshot();
console.log({
	transport: snap.transport(),
	running: snap.running(),
	connected: snap.transport_connected(),
	dispatched: snap.dispatched_requests(),
	sent: snap.sent_frames(),
	received: snap.received_frames(),
	hasError: snap.last_error_present(),
});

const lastErr = tcpServer.last_error_message();
if (lastErr !== undefined) {
	console.warn("server last error:", lastErr);
	tcpServer.clear_last_error();
}
```

### Example Web Pages
Use the browser examples under `mbus-ffi/examples`:
- `network_smoke.html` (WebSocket/TCP path)
- `serial_smoke.html` (Web Serial path, full serial API smoke runner)
- `network_server_smoke.html` (WASM TCP server lifecycle + dispatch + frame passthrough)
- `serial_server_smoke.html` (WASM Serial server lifecycle + dispatch + frame passthrough)

Serve the examples over localhost:
```bash
cd mbus-ffi
python3 -m http.server 8089
```

## Supported Modbus Operations
Client-side WASM/FFI wrappers expose internal client services configured by feature flags:
- `coils`: read single/multiple, write single/multiple
- `registers`: read holding/input, write single/multiple, mask write, read-write multiple
- `discrete-inputs`: read single/multiple
- `fifo`: read
- `file-record`: read/write
- `diagnostics`: exception status, diagnostics, comm event counter/log, report server id, read device ID
- `full`: Enables all Modbus service features.

## WASM Test Coverage

WASM browser tests are in `mbus-ffi/tests/wasm_e2e.rs` and cover:
- Promise behavior and typed payload mapping for client APIs.
- WASM server lifecycle/dispatch surface (`WasmTcpServer`, `WasmSerialServer`).
- Adapter passthrough to transport crates (`mbus-network`, `mbus-serial`).

Run wasm-target checks:

```bash
cargo check -p mbus-ffi --target wasm32-unknown-unknown --features wasm
cargo check -p mbus-ffi --target wasm32-unknown-unknown --features wasm,full
```

Run browser E2E tests:

```bash
bash mbus-ffi/scripts/run_wasm_browser_tests.sh
```

Prerequisites for browser E2E test runs:

- `wasm-pack` installed and available on `PATH`
- Chromium-based browser installed (`google-chrome`, `chromium`, or `chromium-browser`)
- `chromedriver` installed on `PATH` with major version matching local browser

Notes:

- The script enforces these prerequisites before executing tests.
- Canonical command target is fixed to `wasm-pack test --headless --chrome --features wasm,full --test wasm_e2e`.

## Licensing

Copyright (C) 2025 Raghava Challari

This project is currently licensed under the GNU General Public License v3.0 (GPLv3).

For details, refer to the [LICENSE](../LICENSE) file or the [GPLv3 official site](https://www.gnu.org/licenses/gpl-3.0.en.html).

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

## Contact

**Name:** Raghava Ch  
**Email:** [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)  
**Repository:** [github.com/Raghava-Ch/modbus-rs](https://github.com/Raghava-Ch/modbus-rs)
