# Client Examples Reference

All client examples with descriptions and run commands.

---

## TCP Examples

### Read/Write Coils

Demonstrates FC01 (Read Coils), FC05 (Write Single Coil), FC0F (Write Multiple Coils).

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- <host> <port> <unit_id>
```

**Example:**
```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 192.168.1.10 502 1
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/coils.rs](../../modbus-rs/examples/client/network-tcp/sync/coils.rs)

---

### Read/Write Registers

Demonstrates FC03 (Read Holding Registers), FC04 (Read Input Registers), FC06 (Write Single Register), FC10 (Write Multiple Registers).

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_registers -- <host> <port> <unit_id>
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/registers.rs](../../modbus-rs/examples/client/network-tcp/sync/registers.rs)

---

### Read Discrete Inputs

Demonstrates FC02 (Read Discrete Inputs).

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_discrete_inputs -- <host> <port> <unit_id>
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/discrete_inputs.rs](../../modbus-rs/examples/client/network-tcp/sync/discrete_inputs.rs)

---

### Device Identification

Demonstrates FC2B (Read Device Identification / MEI 0x0E).

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_device_id -- <host> <port> <unit_id>
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/device_id.rs](../../modbus-rs/examples/client/network-tcp/sync/device_id.rs)

---

### Backoff and Jitter

Demonstrates configurable retry policies with exponential backoff and jitter.

```bash
cargo run -p modbus-rs --example modbus_rs_client_tcp_backoff_jitter -- <host> <port> <unit_id>
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/backoff_jitter.rs](../../modbus-rs/examples/client/network-tcp/sync/backoff_jitter.rs)

---

### Logging

Demonstrates `log` facade integration for transport diagnostics.

```bash
RUST_LOG=debug cargo run -p modbus-rs --example modbus_rs_client_tcp_logging \
    --no-default-features --features network-tcp,logging
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/logging.rs](../../modbus-rs/examples/client/network-tcp/sync/logging.rs)

---

### Traffic Observability (Sync)

Demonstrates raw TX/RX frame callbacks in sync mode.

```bash
cargo run -p modbus-rs --example modbus_rs_client_traffic_sync_tcp --features traffic
```

**Source:** [modbus-rs/examples/client/network-tcp/sync/traffic.rs](../../modbus-rs/examples/client/network-tcp/sync/traffic.rs)

---

## Async TCP Examples

### Async TCP Client

Demonstrates AsyncTcpClient with Tokio runtime.

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_tcp --features async
```

**Source:** [modbus-rs/examples/client/network-tcp/async/tcp.rs](../../modbus-rs/examples/client/network-tcp/async/tcp.rs)

---

### Traffic Observability (Async)

Demonstrates raw TX/RX frame callbacks in async mode.

```bash
cargo run -p modbus-rs --example modbus_rs_client_traffic_async_tcp --features async,traffic
```

**Source:** [modbus-rs/examples/client/network-tcp/async/traffic.rs](../../modbus-rs/examples/client/network-tcp/async/traffic.rs)

---

## Serial RTU Examples

### Read/Write Coils (Serial)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_coils \
    --no-default-features --features client,serial-rtu,coils -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-rtu/sync/coils.rs](../../modbus-rs/examples/client/serial-rtu/sync/coils.rs)

---

### Read/Write Registers (Serial)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_registers \
    --no-default-features --features client,serial-rtu,registers -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-rtu/sync/registers.rs](../../modbus-rs/examples/client/serial-rtu/sync/registers.rs)

---

### Read Discrete Inputs (Serial)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_discrete_inputs \
    --no-default-features --features client,serial-rtu,discrete-inputs -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-rtu/sync/discrete_inputs.rs](../../modbus-rs/examples/client/serial-rtu/sync/discrete_inputs.rs)

---

### Device Identification (Serial)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_device_id \
    --no-default-features --features client,serial-rtu,diagnostics -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-rtu/sync/device_id.rs](../../modbus-rs/examples/client/serial-rtu/sync/device_id.rs)

---

### Backoff and Jitter (Serial)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_rtu_backoff_jitter \
    --no-default-features --features client,serial-rtu,coils -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-rtu/sync/backoff_jitter.rs](../../modbus-rs/examples/client/serial-rtu/sync/backoff_jitter.rs)

---

### Async Serial RTU

```bash
cargo run -p modbus-rs --example modbus_rs_client_async_serial_rtu \
    --no-default-features --features async,serial-rtu,coils,registers
```

**Source:** [modbus-rs/examples/client/serial-rtu/async/rtu.rs](../../modbus-rs/examples/client/serial-rtu/async/rtu.rs)

---

## Serial ASCII Examples

### Read/Write Coils (ASCII)

```bash
cargo run -p modbus-rs --example modbus_rs_client_serial_ascii_coils \
    --no-default-features --features client,serial-ascii,coils -- /dev/ttyUSB0 1
```

**Source:** [modbus-rs/examples/client/serial-ascii/sync/coils.rs](../../modbus-rs/examples/client/serial-ascii/sync/coils.rs)

---

## Feature Showcase

### All Feature Facades

Demonstrates all function-code services in one example.

<!-- validate: skip -->
```bash
cargo run -p modbus-rs --example modbus_rs_client_showcase_feature_facades \
    --no-default-features --features client,network-tcp,coils,registers,discrete-inputs,diagnostics,fifo,file-record
```

---

## Running Against a Simulator

If you don't have hardware, use a Modbus simulator:

```bash
# Use any Modbus TCP simulator on port 502

# Run example against localhost
cargo run -p modbus-rs --example modbus_rs_client_tcp_coils -- 127.0.0.1 502 1
```

---

## See Also

- [Sync Development](sync.md) — Full poll-driven development guide
- [Feature Flags](feature_flags.md) — Customize your build
- [Async Development](async.md) — Tokio async APIs
