//! Async Modbus serial client.
//!
//! [`AsyncSerialClient`] is a thin wrapper around [`AsyncClientCore`] that adds
//! serial-specific constructors (RTU, ASCII, and injection of a custom
//! transport).  All Modbus request methods are inherited transparently through
//! the [`std::ops::Deref`] implementation that resolves to `AsyncClientCore`.
//!
//! # Note on pipeline depth
//!
//! Serial Modbus is a strict request-reply protocol, so `ClientServices` is
//! always built with a pipeline depth of 1 (`ClientServices::<_, _, 1>`).

use std::ops::Deref;
use super::*;

/// Async Modbus serial client facade.
///
/// Supports both RTU and ASCII framing.  All Modbus request methods
/// (`read_holding_registers`, `write_single_coil`, etc.) are available directly
/// on this type via [`Deref`] to [`AsyncClientCore`].
pub struct AsyncSerialClient {
	core: AsyncClientCore,
}

impl Deref for AsyncSerialClient {
	type Target = AsyncClientCore;

	fn deref(&self) -> &Self::Target {
		&self.core
	}
}

// ── Constructors ─────────────────────────────────────────────────────────────────────

impl AsyncSerialClient {
	/// Creates an async Modbus RTU serial client.
	///
	/// Validates that `serial_config.mode` is [`SerialMode::Rtu`].  Uses a
	/// 20 ms polling interval.
	#[cfg(feature = "serial-rtu")]
	pub fn connect_rtu(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
		if serial_config.mode != SerialMode::Rtu {
			return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
		}

		let transport = StdSerialTransport::new(SerialMode::Rtu);
		let config = ModbusConfig::Serial(serial_config);
		Self::from_transport_config(transport, config, Duration::from_millis(20))
	}

	/// Creates an async Modbus RTU serial client with a custom `poll_interval`.
	///
	/// Validates that `serial_config.mode` is [`SerialMode::Rtu`].
	#[cfg(feature = "serial-rtu")]
	pub fn connect_rtu_with_poll_interval(
		serial_config: ModbusSerialConfig,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		if serial_config.mode != SerialMode::Rtu {
			return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
		}

		let transport = StdSerialTransport::new(SerialMode::Rtu);
		let config = ModbusConfig::Serial(serial_config);
		Self::from_transport_config(transport, config, poll_interval)
	}

	/// Creates an async Modbus ASCII serial client.
	///
	/// Validates that `serial_config.mode` is [`SerialMode::Ascii`].  Uses a
	/// 20 ms polling interval.
	#[cfg(feature = "serial-ascii")]
	pub fn connect_ascii(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
		if serial_config.mode != SerialMode::Ascii {
			return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
		}

		let transport = StdSerialTransport::new(SerialMode::Ascii);
		let config = ModbusConfig::Serial(serial_config);
		Self::from_transport_config(transport, config, Duration::from_millis(20))
	}

	/// Creates an async Modbus ASCII serial client with a custom `poll_interval`.
	///
	/// Validates that `serial_config.mode` is [`SerialMode::Ascii`].
	#[cfg(feature = "serial-ascii")]
	pub fn connect_ascii_with_poll_interval(
		serial_config: ModbusSerialConfig,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		if serial_config.mode != SerialMode::Ascii {
			return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
		}

		let transport = StdSerialTransport::new(SerialMode::Ascii);
		let config = ModbusConfig::Serial(serial_config);
		Self::from_transport_config(transport, config, poll_interval)
	}

	/// Creates an async serial client from a caller-provided transport.
	///
	/// This is the escape hatch for custom serial drivers and integration tests
	/// that inject a mock transport.  The `config` must be
	/// `ModbusConfig::Serial(_)` or the call returns
	/// `AsyncError::Mbus(MbusError::InvalidTransport)`.
	pub fn connect_with_transport<TRANSPORT>(
		transport: TRANSPORT,
		config: ModbusConfig,
		poll_interval: Duration,
	) -> Result<Self, AsyncError>
	where
		TRANSPORT: Transport + Send + 'static,
	{
		if !matches!(config, ModbusConfig::Serial(_)) {
			return Err(AsyncError::Mbus(MbusError::InvalidTransport));
		}

		let pending = Arc::new(Mutex::new(HashMap::new()));
		let app = AsyncApp {
			pending: pending.clone(),
		};

		// Serial is always single-in-flight (pipeline depth 1).
		let client = ClientServices::<_, _, 1>::new(transport, app, config)?;
		let (sender, receiver) = mpsc::channel();

		thread::spawn(move || run_worker(client, pending, receiver, poll_interval));

		Ok(Self {
			core: AsyncClientCore::new(sender),
		})
	}

	/// Internal constructor used by the RTU/ASCII helpers.
	#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
	fn from_transport_config(
		transport: StdSerialTransport,
		config: ModbusConfig,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		Self::connect_with_transport(transport, config, poll_interval)
	}
}
