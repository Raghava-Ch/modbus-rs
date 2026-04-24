# integration_tests

Cross-crate integration tests for the `modbus-rs` workspace. All tests are
self-contained and run without physical hardware — no real serial ports or
external Modbus devices are required.

---

## Test modules

| Module | Transport mechanism | Hardware needed? | Feature flag(s) required |
|---|---|---|---|
| [`tcp_tests`](src/tcp_tests.rs) | `std::net::TcpListener` loopback (`127.0.0.1:0`) | No | default |
| [`async_tests`](src/async_tests.rs) | `TcpListener` loopback, async client | No | default |
| [`serial_tests`](src/serial_tests.rs) | In-process `MockTransport` (no port opened) | No | default |
| [`async_serial_tests`](src/async_serial_tests.rs) | In-process `MockAsyncSerialTransport` | No | default |
| [`server_tests`](src/server_tests.rs) | In-process mock transport, sync `ServerServices` | No | default |
| [`server_over_std_transport_tests`](src/server_over_std_transport_tests.rs) | `TcpListener` loopback, sync client + server | No | default |
| [`async_server_tests`](src/async_server_tests.rs) | Real `AsyncTcpServer` on a random port, loopback | No | `async-server` |

---

## Running the tests

### Default suite (all modules except `async_server_tests`)

```
cargo test -p integration_tests
```

### Include async server tests

```
cargo test -p integration_tests --features async-server
```

### Include traffic-aware variants

```
cargo test -p integration_tests --features async-traffic
```

`async-traffic` implies `async-server` and also enables the `traffic` feature
on both `modbus-rs` and `mbus-server`, exercising the optional traffic-notifier
trait surface.

---

## Mock infrastructure

[`src/mock_app.rs`](src/mock_app.rs) provides `MockApp` — a minimal
`#[modbus_app]`-annotated struct used by the sync server tests.

The serial test modules each define their own in-process mock transport
(`MockTransport` / `MockAsyncSerialTransport`) that captures sent frames and
allows injecting pre-canned responses without opening a real serial port.
