# modbus-rs

A cross-platform, low-footprint Modbus client and server library for Rust.

- **no_std compatible** — runs on embedded MCUs and standard OS targets
- **All transports** — TCP, Serial RTU, Serial ASCII
- **Sync and async** — poll-driven sync core; native `async/await` via Tokio
- **Feature-gated** — enable only what you need for minimal binary size
- **C and WASM bindings** — native C/C++ and browser integration via `mbus-ffi`
- **Gateway** — Modbus TCP ↔ RTU/ASCII gateway with sync (no_std) and async modes

---

## Quick Start

```toml
[dependencies]
modbus-rs = "0.8.0"
```

```rust
use modbus_rs::{ClientServices, ModbusConfig, ModbusTcpConfig, StdTcpTransport};

let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
let mut client = ClientServices::<_, _, 4>::new(StdTcpTransport::new(), app, config)?;
client.connect()?;
client.coils().read_coils(1, unit_id, 0, 16)?;
loop { client.poll(); }
```

## Minimal Install Profiles

Use `default-features = false` and opt into only the features you need.

### Minimal TCP Client

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = ["client", "network-tcp", "coils"] }
```

### Minimal Embedded / no_std Client

```toml
[dependencies]
modbus-rs = { version = "0.8.0", default-features = false, features = ["client", "coils", "registers"] }
```

### Core-only (protocol + models)

```toml
[dependencies]
mbus-core = { version = "0.8.0", default-features = false, features = ["coils", "registers"] }
```

📖 **[Full Documentation →](documentation/README.md)**

---

## Documentation

| Section | Quick Links |
|---------|-------------|
| **Client** | [Quick Start](documentation/client/quick_start.md) · [Examples](documentation/client/examples.md) · [Building Apps](documentation/client/building_applications.md) · [Sync](documentation/client/sync.md) · [Async](documentation/client/async.md) · [Policies](documentation/client/policies.md) |
| **Server** | [Quick Start](documentation/server/quick_start.md) · [Examples](documentation/server/examples.md) · [Building Apps](documentation/server/building_applications.md) · [Sync](documentation/server/sync.md) · [Async](documentation/server/async.md) · [Macros](documentation/server/macros.md) · [Write Hooks](documentation/server/write_hooks.md) · [Function Codes](documentation/server/function_codes.md) |
| **Gateway** | [Quick Start](documentation/gateway/quick_start.md) · [Architecture](documentation/gateway/architecture.md) · [Routing](documentation/gateway/routing.md) · [WebSocket Gateway](documentation/gateway/ws_gateway.md) · [Feature Flags](documentation/gateway/feature_flags.md) |
| **Bindings** | [C/FFI](documentation/client/c_bindings.md) · [WASM](documentation/client/wasm.md) · [Python](documentation/python_bindings.md) |
| **Reference** | [Client Feature Flags](documentation/client/feature_flags.md) · [Server Feature Flags](documentation/server/feature_flags.md) · [Migration Guide](documentation/migration_guide.md) |

---

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| [`modbus-rs`](modbus-rs/) | Top-level convenience crate — start here |
| [`mbus-client`](mbus-client/) | Client state machine and request services |
| [`mbus-server`](mbus-server/) | Server runtime with derive macros |
| [`mbus-client-async`](mbus-client-async/) | Role-focused async client facade crate |
| [`mbus-server-async`](mbus-server-async/) | Role-focused async server facade crate |
| [`mbus-core`](mbus-core/) | Shared protocol types and transport trait |
| [`mbus-async`](mbus-async/) | Combined async client+server crate (still supported) |
| [`mbus-macros`](mbus-macros/) | Proc macros: `#[modbus_app]`, `#[derive(CoilsModel)]`, etc. |
| [`mbus-network`](mbus-network/) | TCP transport implementation |
| [`mbus-serial`](mbus-serial/) | Serial RTU/ASCII transport implementation |
| [`mbus-gateway`](mbus-gateway/) | Modbus gateway — TCP ↔ RTU/ASCII routing (sync + async) |
| [`mbus-ffi`](mbus-ffi/) | Native C and WASM bindings (client + phased server surface) |

### Direct Async Crate Selection

- Use [`mbus-client-async`](mbus-client-async/) for async client-only dependencies.
- Use [`mbus-server-async`](mbus-server-async/) for async server-only dependencies.
- Use [`mbus-async`](mbus-async/) if you want a combined async client+server dependency.

---

## Feature Flags

| Flag | Description |
|------|-------------|
| `client` | Client state machine (default) |
| `server` | Server runtime and macros |
| `network-tcp` | Modbus TCP transport (default) |
| `serial-rtu` | Serial RTU transport (default) |
| `serial-ascii` | Serial ASCII transport |
| `async` | Native async runtime via Tokio for client and server APIs (default) |
| `coils` | FC01, FC05, FC0F (default) |
| `registers` | FC03, FC04, FC06, FC10 (default) |
| `discrete-inputs` | FC02 (default) |
| `fifo` | FC18 FIFO queue read (default) |
| `file-record` | FC14, FC15 file record read/write (default) |
| `diagnostics` | FC07, FC08, FC2B, etc. (default) |
| `gateway` | Modbus gateway (TCP ↔ RTU/ASCII, sync + async) |
| `diagnostics-stats` | Per-counter diagnostics statistics |
| `traffic` | Raw TX/RX frame callbacks |
| `logging` | `log` facade integration |

See [Feature Flags Reference](documentation/client/feature_flags.md) for complete details.

---

## Examples

### TCP Client (sync)

```rust
use modbus_rs::{ClientServices, ModbusConfig, ModbusTcpConfig, StdTcpTransport};

let config = ModbusConfig::Tcp(ModbusTcpConfig::new("192.168.1.10", 502)?);
let mut client = ClientServices::<_, _, 4>::new(StdTcpTransport::new(), app, config)?;
client.connect()?;
client.coils().read_coils(1, unit_id, 0, 16)?;
loop { client.poll(); }
```

### Async TCP Client

```rust
use modbus_rs::mbus_async::AsyncTcpClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let client = AsyncTcpClient::new("192.168.1.10", 502)?;
    client.connect().await?;

    let coils = client.read_multiple_coils(1, 0, 8).await?;
    for addr in coils.from_address()..coils.from_address() + coils.quantity() {
        println!("coil[{}] = {}", addr, coils.value(addr)?);
    }

    let holding = client.read_holding_registers(1, 0, 4).await?;
    for addr in holding.from_address()..holding.from_address() + holding.quantity() {
        println!("reg[{}] = {}", addr, holding.value(addr)?);
    }

    client.write_single_coil(1, 0, true).await?;
    Ok(())
}
```

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --no-default-features --features async,client,network-tcp,coils,registers,discrete-inputs
```

### C Client (via `mbus-ffi`)

```c
#include "modbus_rs_client.h"

/* Required locking hooks — provide real mutexes in production */
void mbus_pool_lock(void)        { /* pthread_mutex_lock(&g_pool_mutex); */ }
void mbus_pool_unlock(void)      { /* pthread_mutex_unlock(&g_pool_mutex); */ }
void mbus_client_lock(MbusClientId id)   { (void)id; }
void mbus_client_unlock(MbusClientId id) { (void)id; }

/* Transport callbacks — wire these to your socket/UART layer */
static MbusStatusCode on_connect(void *ud)    { return tcp_open(ud);  }
static MbusStatusCode on_disconnect(void *ud) { return tcp_close(ud); }
static MbusStatusCode on_send(const uint8_t *buf, uint16_t len, void *ud)
    { return tcp_write(ud, buf, len); }
static MbusStatusCode on_recv(uint8_t *buf, uint16_t cap, uint16_t *out, void *ud)
    { return tcp_read(ud, buf, cap, out); }
static uint8_t on_is_connected(void *ud) { return tcp_is_open(ud); }

/* Response callback */
static void on_read_coils(const MbusReadCoilsCtx *ctx) {
    for (uint16_t i = 0; i < mbus_coils_quantity(ctx->coils); i++) {
        bool val; mbus_coils_value_at_index(ctx->coils, i, &val);
        printf("coil[%u] = %d\n", i, val);
    }
}

int main(void) {
    struct MyTcpCtx io = { .fd = -1, .host = "192.168.1.10", .port = 502 };

    MbusTransportCallbacks transport = {
        .userdata = &io, .on_connect = on_connect, .on_disconnect = on_disconnect,
        .on_send = on_send, .on_recv = on_recv, .on_is_connected = on_is_connected,
    };
    MbusTcpConfig cfg = { .host = "192.168.1.10", .port = 502,
                          .response_timeout_ms = 2000, .retries = 1 };
    MbusCallbacks app = { .on_read_coils = on_read_coils };

    MbusClientId id = mbus_tcp_client_new(&cfg, &transport, &app);
    mbus_tcp_connect(id);
    mbus_tcp_read_coils(id, /*unit*/1, /*txn*/42, /*addr*/0, /*qty*/10);

    while (mbus_tcp_has_pending_requests(id))
        mbus_tcp_poll(id);

    mbus_tcp_disconnect(id);
    mbus_tcp_client_free(id);
}
```

See [`mbus-ffi/`](mbus-ffi/) for the full C binding reference, build instructions, and server demo.

### WASM Browser Bindings (via `mbus-ffi`)

Build the WASM package and serve locally:

```bash
cd mbus-ffi
wasm-pack build --target web --features wasm,full
python3 -m http.server 8089
```

Run canonical browser E2E tests:

```bash
bash mbus-ffi/scripts/run_wasm_browser_tests.sh
```

Open the runnable smoke examples in a Chromium-based browser:

| Example | What it exercises |
|---|---|
| [examples/network_smoke.html](mbus-ffi/examples/wasm_client/network_smoke.html) | WebSocket client (TCP proxy) |
| [examples/serial_smoke.html](mbus-ffi/examples/wasm_client/serial_smoke.html) | Web Serial client (RTU/ASCII) |
| [examples/network_server_smoke.html](mbus-ffi/examples/wasm_server/network_smoke.html) | WASM TCP server lifecycle + dispatch |
| [examples/serial_server_smoke.html](mbus-ffi/examples/wasm_server/serial_smoke.html) | WASM Serial server lifecycle + dispatch |

See [`mbus-ffi/README.md`](mbus-ffi/README.md) for the full WASM API reference and server binding architecture.

### Gateway (sync TCP → RTU)

```rust
use modbus_rs::{gateway::GatewayServices, gateway::UnitRouteTable, gateway::NoopEventHandler,
                gateway::DownstreamChannel};

// Route unit IDs 1–10 to channel 0 (RTU downstream)
let mut routes = UnitRouteTable::new();
routes.add(1, 10, 0).unwrap();

let mut gw = GatewayServices::<_, _, 1, 8>::new(
    upstream_transport, [(downstream_rtu_transport, 0)],
    routes, NoopEventHandler,
);

loop { gw.poll(); }
```

```bash
# Sync TCP → RTU gateway
cargo run -p modbus-rs --example modbus_rs_gateway_sync_tcp_to_rtu \
  --no-default-features --features gateway,network-tcp,serial-rtu

# Async TCP → TCP gateway
cargo run -p modbus-rs --example modbus_rs_gateway_async_tcp_to_tcp \
  --no-default-features --features gateway,async,network-tcp
```

📖 **[Gateway Documentation →](documentation/gateway/quick_start.md)**

---

### Run examples

```bash
# Sync TCP client
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils --no-default-features --features client,network-tcp,coils -- 192.168.1.10 502 1

# Serial RTU client
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_coils --no-default-features --features client,serial-rtu,coils -- /dev/ttyUSB0 1

# TCP server
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo --features server,network-tcp,coils,holding-registers,input-registers
```

📖 **[All Examples →](documentation/client/examples.md)**

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and contribution workflow.

## License

This project is licensed under the **GNU General Public License v3.0 (GPLv3)** — see [LICENSE](LICENSE).

This crate is licensed under GPLv3. If you require a commercial license to use this crate in a proprietary project, please contact [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com) to purchase a license.

---

**Repository:** [github.com/Raghava-Ch/modbus-rs](https://github.com/Raghava-Ch/modbus-rs)