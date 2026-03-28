//! Async Modbus TCP client.
//!
//! [`AsyncTcpClient`] is a thin wrapper around [`AsyncClientCore`] that adds
//! TCP-specific constructors.  All Modbus request methods are inherited
//! transparently through the [`std::ops::Deref`] implementation that resolves
//! to `AsyncClientCore`.

use std::ops::Deref;
use super::*;

/// Async Modbus TCP client facade.
///
/// All Modbus request methods (`read_holding_registers`, `write_single_coil`,
/// etc.) are available directly on this type via [`Deref`] to
/// [`AsyncClientCore`].
///
/// The constant generic parameter `N` is the compile-time pipeline depth
/// forwarded to `ClientServices<_, _, N>` (default `9`).
pub struct AsyncTcpClient<const N: usize = 9> {
	core: AsyncClientCore,
}

impl<const N: usize> Deref for AsyncTcpClient<N> {
	type Target = AsyncClientCore;

	fn deref(&self) -> &Self::Target {
		&self.core
	}
}

// ── Default-pipeline constructors (N = 9) ───────────────────────────────────

impl AsyncTcpClient<9> {
	/// Creates an async TCP client connected to `host`:`port`.
	///
	/// Uses the default pipeline depth of 9 and a 20 ms polling interval.
	#[cfg(feature = "tcp")]
	pub fn connect(host: &str, port: u16) -> Result<Self, AsyncError> {
		Self::connect_with_pipeline(host, port)
	}

	/// Creates an async TCP client connected to `host`:`port` with a custom
	/// `poll_interval`.
	///
	/// Uses the default pipeline depth of 9.
	#[cfg(feature = "tcp")]
	pub fn connect_with_poll_interval(
		host: &str,
		port: u16,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		Self::connect_with_pipeline_and_poll_interval(host, port, poll_interval)
	}
}

// ── Configurable-pipeline constructors ───────────────────────────────────────

impl<const N: usize> AsyncTcpClient<N> {
	/// Creates an async TCP client with compile-time pipeline depth `N`.
	///
	/// Uses a 20 ms polling interval.
	#[cfg(feature = "tcp")]
	pub fn connect_with_pipeline(host: &str, port: u16) -> Result<Self, AsyncError> {
		let transport = StdTcpTransport::new();
		let config = ModbusConfig::Tcp(ModbusTcpConfig::new(host, port)?);
		Self::from_transport_config(transport, config, Duration::from_millis(20))
	}

	/// Creates an async TCP client with compile-time pipeline depth `N` and a
	/// custom `poll_interval`.
	#[cfg(feature = "tcp")]
	pub fn connect_with_pipeline_and_poll_interval(
		host: &str,
		port: u16,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		let transport = StdTcpTransport::new();
		let config = ModbusConfig::Tcp(ModbusTcpConfig::new(host, port)?);
		Self::from_transport_config(transport, config, poll_interval)
	}

	/// Internal constructor: wires `transport` + `config` into a
	/// `ClientServices` instance, spawns the worker thread, and wraps the
	/// resulting channel in an [`AsyncClientCore`].
	#[cfg(feature = "tcp")]
	fn from_transport_config(
		transport: StdTcpTransport,
		config: ModbusConfig,
		poll_interval: Duration,
	) -> Result<Self, AsyncError> {
		let pending = Arc::new(Mutex::new(HashMap::new()));
		let app = AsyncApp {
			pending: pending.clone(),
		};

		let client = ClientServices::<_, _, N>::new(transport, app, config)?;
		let (sender, receiver) = mpsc::channel();

		thread::spawn(move || run_worker(client, pending, receiver, poll_interval));

		Ok(Self {
			core: AsyncClientCore::new(sender),
		})
	}
}
