# WASM Client Development

Browser-based Modbus client using WebAssembly and WebSocket.

---

## Overview

The `mbus-ffi` crate compiles to WebAssembly for browser-based Modbus communication over WebSocket.

**Use cases:**
- Web HMI dashboards
- Browser-based SCADA interfaces
- IoT monitoring applications
- No server-side installation required

---

## Prerequisites

1. Install wasm-pack:

```bash
cargo install wasm-pack
```

2. Build the WASM package:

```bash
cd mbus-ffi
wasm-pack build --target web
```

3. Find the output in `mbus-ffi/pkg/`:
   - `mbus_ffi.js` — JavaScript bindings
   - `mbus_ffi_bg.wasm` — WebAssembly module
   - `mbus_ffi.d.ts` — TypeScript declarations

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Browser                                                    │
│  ┌─────────────────┐      ┌────────────────────────────┐    │
│  │  Your Web App   │─────▶│  mbus_ffi.js               │    │
│  │  (HTML/JS/TS)   │      │  (WASM wrapper)            │    │
│  └─────────────────┘      └────────────────────────────┘    │
│                                      │                      │
│                                      ▼                      │
│                           ┌────────────────────────────┐    │
│                           │  WebSocket Transport       │    │
│                           │  ws://proxy:8080           │    │
│                           └────────────────────────────┘    │
└─────────────────────────────────────────────────────────────┘
                                       │
                                       ▼
                            ┌────────────────────┐
                            │  WebSocket Proxy   │
                            └────────────────────┘
                                       │
                                       ▼
                            ┌────────────────────┐
                            │  Modbus Device     │
                            │  (TCP port 502)    │
                            └────────────────────┘
```

**Note:** Browsers cannot make raw TCP connections. A WebSocket-to-TCP proxy is required.

---

## Quick Start

### 1. Include WASM Module

```html
<!DOCTYPE html>
<html>
<head>
    <title>Modbus WASM Client</title>
</head>
<body>
    <div id="output"></div>
    
    <script type="module">
        import init, { WasmModbusClient } from './pkg/mbus_ffi.js';
        
        async function run() {
            // Initialize WASM module
            await init();
            
            // Create client connected via WebSocket
            const client = new WasmModbusClient("ws://localhost:8080", 1, 2000, 1, 20);

            // Requests return Promises with typed payloads.
            const coils = await client.read_coils(0, 16);
            console.log("Coils (packed bytes):", Array.from(coils));
            document.getElementById("output").textContent = JSON.stringify(Array.from(coils));

            // Optional health/status checks.
            console.log("connected:", client.is_connected());
            console.log("pending:", client.has_pending_requests());
        }
        
        run();
    </script>
</body>
</html>
```

### 2. Run WebSocket Proxy

Use the included proxy script:

```bash
node mbus-ffi/examples/proxy.js
```

Or use any WebSocket-to-TCP proxy. Example nginx config:

```nginx
stream {
    upstream modbus {
        server 192.168.1.10:502;
    }
    
    server {
        listen 8080;
        proxy_pass modbus;
    }
}

http {
    server {
        listen 80;
        
        location /ws {
            proxy_pass http://127.0.0.1:8080;
            proxy_http_version 1.1;
            proxy_set_header Upgrade $http_upgrade;
            proxy_set_header Connection "upgrade";
        }
    }
}
```

---

## TypeScript Usage

```typescript
import init, { WasmModbusClient } from './pkg/mbus_ffi';

async function main() {
    await init();
    
    const client = new WasmModbusClient("ws://localhost:8080", 1, 2000, 1, 20);

    // Periodic polling
    setInterval(async () => {
        try {
            const regs = await client.read_holding_registers(0, 10);
            console.log("holding registers:", Array.from(regs));
        } catch (err) {
            console.error("request failed:", err);
        }
    }, 1000);
}

main();
```

---

## Web Serial (Browser Serial API)

In addition to WebSocket/TCP proxy mode, the wasm bindings also expose Web Serial
support through:

- `request_serial_port()`
- `WasmSerialPortHandle`
- `WasmSerialModbusClient`

Typical flow:

1. Call `request_serial_port()` from a user gesture (button click).
2. Construct `WasmSerialModbusClient` with the returned handle.
3. Use the same Promise-based request methods (`read_coils`, `read_holding_registers`, etc.).

Web Serial is currently supported in Chromium-based browsers under secure contexts
(`https://` or `http://localhost`).

---

## WASM Server Bindings (Phase 1/2 Surface)

`mbus-ffi` now exposes browser-facing server binding types:

- `WasmTcpServer` + `WasmTcpGatewayConfig`
- `WasmSerialServer` + `WasmSerialServerConfig`

Current scope:

- Lifecycle controls: `start()`, `stop()`, `is_running()`
- Request bridge: `dispatch_request(...)` through JS callback handler
- Adapter passthrough helpers: `send_frame(...)`, `recv_frame(...)`
- WebSocket handshake helpers on `WasmTcpServer`: `transport_connecting()` and `transport_connected()` (`transport_connected()` is true only after websocket OPEN)

### Supported Protocol Surface (Contract)

Current contractual server binding support:

- Lifecycle APIs are stable (`start`, `stop`, `is_running`)
- `dispatch_request(...)` callback bridge is stable (sync return or Promise return)
- Raw frame transport passthrough helpers are stable (`send_frame`, `recv_frame`)

Not currently contractual at server binding level:

- Built-in Modbus FC request parsing/routing by `WasmTcpServer` / `WasmSerialServer`
- Guaranteed typed FC helper APIs on server bindings
- End-to-end managed protocol loop in `mbus-ffi` server layer

Planned expansion (non-contractual roadmap intent):

- Incremental typed protocol helpers and FC mapping on top of current bridge
- More managed request/response orchestration while preserving transport ownership boundaries

### Transport Ownership Boundary

WASM transport implementations are not reimplemented in `mbus-ffi` server bindings:

- `mbus-network` owns websocket WASM transport implementation
- `mbus-serial` owns Web Serial WASM transport implementation

`mbus-ffi` owns only binding orchestration (lifecycle + JS bridge + adapter wiring).

Note:

- Any behavior implemented only in example pages (for smoke/demo convenience) is not part of the stable binding contract unless documented in the API sections above.

---

## API Reference

### Constructor

```typescript
new WasmModbusClient(
    ws_url: string,
    unit_id: number,
    response_timeout_ms: number,
    retry_attempts: number,
    tick_interval_ms: number
)
```

### Methods

| Method | Description |
|--------|-------------|
| `is_connected()` | Check connection status |
| `has_pending_requests()` | Returns `true` while requests are in flight |
| `reconnect()` | Reconnect underlying transport and fail in-flight requests |
| `read_coils(address, quantity)` | FC01 |
| `read_discrete_inputs(address, quantity)` | FC02 |
| `read_holding_registers(address, quantity)` | FC03 |
| `read_input_registers(address, quantity)` | FC04 |
| `write_single_coil(address, value)` | FC05 |
| `write_single_register(address, value)` | FC06 |
| `write_multiple_coils(address, quantity, values)` | FC0F |
| `write_multiple_registers(address, quantity, values)` | FC10 |

### Promise-based Responses

```typescript
const regs = await client.read_holding_registers(0, 10);
console.log(Array.from(regs));

if (client.has_pending_requests()) {
    console.log("requests still in flight");
}
```

Errors are surfaced by Promise rejection (for example: connection loss, timeout/retry exhaustion, or protocol-level failures).

---

## Example HTML Smoke Test

A complete example is available at:

[mbus-ffi/examples/network_smoke.html](../../mbus-ffi/examples/wasm_client/network_smoke.html)

```bash
# Serve the example
cd mbus-ffi/examples
python3 -m http.server 8000

# Open browser
open http://localhost:8000/network_smoke.html
```

---

## Build Variants

### Standard (TCP via WebSocket)

```bash
wasm-pack build --target web
```

### With All Features

```bash
wasm-pack build --target web --features "coils,registers,discrete-inputs,diagnostics"
```

---

## Security Considerations

1. **WebSocket URL** — Use `wss://` in production
2. **Proxy Authentication** — Implement at proxy level
3. **CORS** — Configure proxy for allowed origins
4. **CSP** — Allow `'wasm-unsafe-eval'` for WASM

---

## Troubleshooting

### WASM Module Fails to Load

```
CompileError: WebAssembly.instantiate()
```

Ensure your server sends correct MIME types:
- `.wasm` → `application/wasm`
- `.js` → `application/javascript`

### WebSocket Connection Refused

Check that:
1. Proxy is running on the correct port
2. No firewall blocking connection
3. Correct WebSocket URL (ws:// vs wss://)

### Responses Not Arriving

1. Check browser DevTools Network tab for WebSocket frames
2. Verify Modbus device is responding (test with a TCP client first)
3. Check proxy logs for errors

---

## See Also

- [C/FFI Bindings](c_bindings.md) — Native C integration
- [Feature Flags](feature_flags.md) — Build options
- [Sync Development](sync.md) — Rust sync client guide
