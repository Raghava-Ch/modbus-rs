# mbus-serial

`mbus-serial` is a helper crate for [modbus-rs](https://crates.io/crates/modbus-rs).

It provides a standard serial transport implementation for Modbus RTU and Modbus ASCII,
built on top of the shared transport abstractions in `mbus-core`.

If you want an all-in-one entry point, use `modbus-rs`.
If you need direct access to serial transport internals, use `mbus-serial` directly.

## Helper Crate Role

`mbus-serial` is intentionally focused on transport concerns:

- Implements `Transport` from `mbus-core`.
- Connects to real serial ports via the `serialport` crate.
- Supports both RTU (`StdRtuTransport`) and ASCII (`StdAsciiTransport`) modes.

This crate does not implement high-level request/response orchestration by itself.
That logic lives in `mbus-client`.

## What Is Included

- `StdRtuTransport` / `StdAsciiTransport`: concrete serial implementations of `modbus_rs::Transport`.
- Serial connection handling (open/close/check connection).
- ADU send/receive support.
- Error mapping from I/O errors to `TransportError`.
- Utility function to enumerate serial ports.

## Public API Surface

The crate currently re-exports:

- `StdSerialTransport`, `StdRtuTransport`, `StdAsciiTransport`

from:

- `management::std_serial`

## Usage

### 1) Add dependencies

```toml
[dependencies]
modbus-rs = "0.5.0"
```

### 2) Create serial config and transport

```rust
use modbus_rs::{
	BackoffStrategy, BaudRate, DataBits, JitterStrategy, MbusError, ModbusConfig,
	ModbusSerialConfig, Parity, SerialMode,
	StdRtuTransport, Transport,
};

fn connect_serial() -> Result<(), MbusError> {
	let config = ModbusConfig::Serial(ModbusSerialConfig {
		port_path: "/dev/ttyUSB0".try_into().map_err(|_| MbusError::BufferTooSmall)?,
		mode: SerialMode::Rtu,
		baud_rate: BaudRate::Baud19200,
		data_bits: DataBits::Eight,
		stop_bits: 1,
		parity: Parity::Even,
		response_timeout_ms: 1000,
		retry_attempts: 3,
		retry_backoff_strategy: BackoffStrategy::Immediate,
		retry_jitter_strategy: JitterStrategy::None,
		retry_random_fn: None,
	});

	let mut transport = StdRtuTransport::new();
	transport.connect(&config)?;

	// send/recv calls are used by higher-level client code

	transport.disconnect()?;
	Ok(())
}
```

### 3) List available serial ports

```rust
use modbus_rs::StdRtuTransport;

fn list_ports() {
	match StdRtuTransport::available_ports() {
		Ok(ports) => {
			for p in ports {
				println!("{}", p.port_name);
			}
		}
		Err(e) => eprintln!("failed to list ports: {}", e),
	}
}
```

## Configuration Notes

- Use `StdRtuTransport::new()` with `SerialMode::Rtu` configs and `StdAsciiTransport::new()` with `SerialMode::Ascii` configs.
  If they do not match, `connect` returns `TransportError::InvalidConfiguration`.
- `stop_bits` must be `1` or `2`.
- `response_timeout_ms` controls serial read timeout behavior.

## Logging

`mbus-serial` supports optional logging via the `log` facade.

- Enable feature: `logging`
- This only emits through the facade; your application provides a logger backend.

Example dependency setup:

```toml
[dependencies]
mbus-serial = { version = "0.5.0", features = ["logging"] }
env_logger = "0.11"
```

## Platform Notes

- Uses the `serialport` crate under the hood.
- Error behavior can vary by driver/OS.
- Some pseudo-terminals (especially on macOS) may not support all serial parameter operations.

## Typical Integration Pattern

In most applications, `mbus-serial` is used together with `mbus-client`:

1. Build `ModbusConfig::Serial(...)`.
2. Instantiate `StdRtuTransport` or `StdAsciiTransport`.
3. Pass transport into `ClientServices` from `mbus-client`.
4. Use client services for function-code operations.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](../LICENSE) for details.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)