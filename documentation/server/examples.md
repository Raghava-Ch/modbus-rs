# Server Examples Reference

All server examples with descriptions and run commands.

---

## TCP Server Examples

### TCP Demo Server

Demonstrates a TCP Modbus server with derive-model based app wiring.

```bash
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo
```

**Source:** [modbus-rs/examples/server/network-tcp/sync/demo.rs](../../modbus-rs/examples/server/network-tcp/sync/demo.rs)

---

### TCP Shared-State Demo

Demonstrates a TCP server app using shared state patterns.

```bash
cargo run -p modbus-rs --example modbus_rs_server_std_transport_client_demo
```

**Source:** [modbus-rs/examples/server/network-tcp/sync/shared_state.rs](../../modbus-rs/examples/server/network-tcp/sync/shared_state.rs)

---

### TCP FIFO + File Record Demo

Demonstrates a sync server app that routes FC18 through `fifo(...)` and
FC14/FC15 through `file_record(...)` using `#[modbus_app(...)]`.

```bash
cargo run -p modbus-rs --example fifo_file_record_demo \
    --features server,network-tcp,fifo,file-record
```

**Source:** [modbus-rs/examples/server/network-tcp/sync/fifo_file_record_demo.rs](../../modbus-rs/examples/server/network-tcp/sync/fifo_file_record_demo.rs)

---

## Async TCP Server Examples

### Async TCP Demo Server

Demonstrates a shared-state async TCP server using `#[async_modbus_app]` and
`AsyncTcpServer::serve_shared`. A background task simulates live register changes
so connected clients observe updating data.

```bash
cargo run -p modbus-rs --example modbus_rs_server_async_tcp_demo \
    --features server,async,network-tcp,coils,holding-registers,input-registers
```

**Source:** [modbus-rs/examples/server/network-tcp/async/demo.rs](../../modbus-rs/examples/server/network-tcp/async/demo.rs)

---

### Async TCP Traffic Logging

Demonstrates implementing [`AsyncTrafficNotifier`] to intercept all raw ADU frames
(TX, RX, framing errors, transmit errors) and the `on_exception` callback.

```bash
cargo run -p modbus-rs --example modbus_rs_server_async_tcp_traffic \
    --features server,async,network-tcp,coils,holding-registers,traffic
```

**Source:** [modbus-rs/examples/server/network-tcp/async/traffic.rs](../../modbus-rs/examples/server/network-tcp/async/traffic.rs)

---

### Async TCP FIFO + File Record Demo

Demonstrates a full async TCP server that routes FC18 through `fifo(...)` and
FC14/FC15 through `file_record(...)` using `#[async_modbus_app(...)]`, with a
background task that updates the served data.

```bash
cargo run -p modbus-rs --example modbus_rs_server_async_fifo_file_record_demo \
    --features server,async,network-tcp,fifo,file-record
```

**Source:** [modbus-rs/examples/server/network-tcp/async/fifo_file_record_demo.rs](../../modbus-rs/examples/server/network-tcp/async/fifo_file_record_demo.rs)

---

## Serial RTU Server Examples

### Serial RTU Demo

```bash
cargo run -p modbus-rs --example modbus_rs_server_serial_rtu_demo
```

**Source:** [modbus-rs/examples/server/serial-rtu/sync/demo.rs](../../modbus-rs/examples/server/serial-rtu/sync/demo.rs)

---

### Serial RTU Manual App (No Macros)

```bash
cargo run -p modbus-rs --example modbus_rs_server_serial_rtu_manual_no_macros
```

**Source:** [modbus-rs/examples/server/serial-rtu/sync/manual_app_no_macros.rs](../../modbus-rs/examples/server/serial-rtu/sync/manual_app_no_macros.rs)

---

## Serial ASCII Server Examples

### Serial ASCII Demo

```bash
cargo run -p modbus-rs --example modbus_rs_server_serial_ascii_demo \
	--features serial-ascii,coils,holding-registers,input-registers
```

**Source:** [modbus-rs/examples/server/serial-ascii/sync/demo.rs](../../modbus-rs/examples/server/serial-ascii/sync/demo.rs)

---

## Testing with Client Examples

Run a server in one terminal:

```bash
cargo run -p modbus-rs --example modbus_rs_server_tcp_demo
```

And test with a client in another:

```bash
# Read coils (CLI host/port aware)
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter -- 127.0.0.1 5502 1

# Read/write registers
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers -- 127.0.0.1 5502 1
```

Expected server startup output includes:

```text
Modbus TCP demo server listening on 127.0.0.1:5502
Unit id: 1
Supported now: FC01, FC03, FC04, FC05, FC06, FC0F, FC10, FC17
```

Note: `modbus_rs_client_tcp_coils` is currently hardcoded to a fixed host/port and does not use CLI host/port args.

---

## See Also

- [Sync Server Applications](sync.md) — Full poll-driven development guide
- [Feature Flags](feature_flags.md) — Customize your build
- [Macros](macros.md) — Derive macro reference
