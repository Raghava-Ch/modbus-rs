# mbus-tcp

`mbus-tcp`  is a helper crate for [modbus-rs](https://crates.io/crates/modbus-rs)..

It provides a standard Modbus TCP transport implementation that plugs into the
shared transport abstractions from `mbus-core`.

If you want a single top-level API, use `modbus-rs`.
If you need direct transport-level control, use `mbus-tcp` directly.

## Helper Crate Role

`mbus-tcp` is transport-focused and intentionally small:

- Implements `Transport` from `mbus-core` using `std::net::TcpStream`.
- Handles connection setup, send, receive, and disconnect for Modbus TCP.
- Maps I/O failures into `TransportError`.

This crate does not implement request orchestration or function-code services.
That logic is provided by `modbus-client`.

## What Is Included

- `StdTcpTransport`: concrete transport implementation for Modbus TCP.
- Timeout setup from `ModbusTcpConfig`.
- Basic DNS resolution and connect flow.
- Non-blocking receive pass that returns currently available bytes.

## Public API Surface

The crate currently re-exports:

- `StdTcpTransport`

from:

- `management::std_transport`

## Usage

### 1) Add dependencies

```toml
[dependencies]
mbus-core = "0.1.0"
mbus-tcp = "0.1.0"
```

### 2) Create TCP config and connect transport

```rust
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, ModbusTcpConfig, Transport};
use mbus_tcp::StdTcpTransport;

fn connect_tcp() -> Result<(), MbusError> {
		let config = ModbusConfig::Tcp(ModbusTcpConfig::new("127.0.0.1", 502)?);

		let mut transport = StdTcpTransport::new();
		transport.connect(&config)?;

		// send/recv calls are typically driven by higher-level client logic

		transport.disconnect()?;
		Ok(())
}
```

## Configuration Notes

- Use `ModbusConfig::Tcp(...)` when calling `connect`.
	Passing a serial config returns a transport configuration error.
- `connection_timeout_ms` and `response_timeout_ms` from `ModbusTcpConfig` are
	applied to the underlying stream.
- `keep_alive_interval_ms` exists in core config, but keep-alive is not currently
	enabled in `StdTcpTransport`.

## Receive Behavior

- `recv()` returns bytes currently available from the socket.
- A full Modbus ADU may arrive in multiple chunks.
- Higher-level logic should buffer and frame messages as needed.

## Typical Integration Pattern

In most applications, `mbus-tcp` is used together with `modbus-client`:

1. Build `ModbusConfig::Tcp(...)`.
2. Instantiate `StdTcpTransport`.
3. Pass transport into `ClientServices` from `modbus-client`.
4. Use client services to issue function-code operations.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](./LICENSE) for details.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)