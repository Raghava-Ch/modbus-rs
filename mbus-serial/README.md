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
- Supports both `SerialMode::Rtu` and `SerialMode::Ascii`.

This crate does not implement high-level request/response orchestration by itself.
That logic lives in `modbus-client`.

## What Is Included

- `StdSerialTransport`: concrete serial implementation of `mbus_core::transport::Transport`.
- Serial connection handling (open/close/check connection).
- ADU send/receive support.
- Error mapping from I/O errors to `TransportError`.
- Utility function to enumerate serial ports.

## Public API Surface

The crate currently re-exports:

- `StdSerialTransport`

from:

- `management::std_serial`

## Usage

### 1) Add dependencies

```toml
[dependencies]
mbus-core = "0.1.0"
mbus-serial = "0.1.0"
```

### 2) Create serial config and transport

```rust
use mbus_core::errors::MbusError;
use mbus_core::transport::{
	BaudRate, ModbusConfig, ModbusSerialConfig, Parity, SerialMode, Transport,
};
use mbus_serial::StdSerialTransport;

fn connect_serial() -> Result<(), MbusError> {
	let config = ModbusConfig::Serial(ModbusSerialConfig {
		port_path: "/dev/ttyUSB0".try_into().map_err(|_| MbusError::BufferTooSmall)?,
		mode: SerialMode::Rtu,
		baud_rate: BaudRate::Baud19200,
		data_bits: 8,
		stop_bits: 1,
		parity: Parity::Even,
		response_timeout_ms: 1000,
		retry_attempts: 3,
	});

	let mut transport = StdSerialTransport::new(SerialMode::Rtu);
	transport.connect(&config)?;

	// send/recv calls are used by higher-level client code

	transport.disconnect()?;
	Ok(())
}
```

### 3) List available serial ports

```rust
use mbus_serial::StdSerialTransport;

fn list_ports() {
	match StdSerialTransport::available_ports() {
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

- `StdSerialTransport::new(mode)` must match the mode in `ModbusSerialConfig`.
  If they do not match, `connect` returns `TransportError::InvalidConfiguration`.
- `stop_bits` must be `1` or `2`.
- `response_timeout_ms` controls serial read timeout behavior.

## Platform Notes

- Uses the `serialport` crate under the hood.
- Error behavior can vary by driver/OS.
- Some pseudo-terminals (especially on macOS) may not support all serial parameter operations.

## Typical Integration Pattern

In most applications, `mbus-serial` is used together with `modbus-client`:

1. Build `ModbusConfig::Serial(...)`.
2. Instantiate `StdSerialTransport`.
3. Pass transport into `ClientServices` from `modbus-client`.
4. Use client services for function-code operations.

## License

Copyright (C) 2025 Raghava Challari

This project is currently licensed under GNU GPL v3.0.
See [LICENSE](./LICENSE) for details.

## Contact

For questions or support:

- Name: Raghava Ch
- Email: [ch.raghava44@gmail.com](mailto:ch.raghava44@gmail.com)