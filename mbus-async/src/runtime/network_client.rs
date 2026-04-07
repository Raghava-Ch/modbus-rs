//! Async Modbus TCP client.
//!
//! [`AsyncTcpClient`] is a thin wrapper around [`AsyncClientCore`] that adds
//! TCP-specific constructors. All Modbus request methods are inherited
//! transparently through the [`std::ops::Deref`] implementation that resolves
//! to `AsyncClientCore`.

use super::*;
use std::ops::Deref;

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
    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new`] and then call `client.connect().await?`.
    #[cfg(feature = "tcp")]
    #[deprecated(note = "use AsyncTcpClient::new(...) and then client.connect().await")]
    pub fn connect(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new(host, port)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new_with_poll_interval`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "tcp")]
    #[deprecated(
        note = "use AsyncTcpClient::new_with_poll_interval(...) and then client.connect().await"
    )]
    pub fn connect_with_poll_interval(
        host: &str,
        port: u16,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_poll_interval(host, port, poll_interval)
    }

    /// Creates an async TCP client for `host`:`port` without connecting.
    ///
    /// Uses the default pipeline depth of 9 and a 20 ms polling interval. Call
    /// [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "tcp")]
    pub fn new(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Creates an async TCP client for `host`:`port` with a custom
    /// `poll_interval`.
    ///
    /// Uses the default pipeline depth of 9. Call [`AsyncClientCore::connect`]
    /// on the returned client before sending requests.
    #[cfg(feature = "tcp")]
    pub fn new_with_poll_interval(
        host: &str,
        port: u16,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_pipeline_and_poll_interval(host, port, poll_interval)
    }
}

// ── Configurable-pipeline constructors ───────────────────────────────────────

impl<const N: usize> AsyncTcpClient<N> {
    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new_with_pipeline`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "tcp")]
    #[deprecated(
        note = "use AsyncTcpClient::new_with_pipeline(...) and then client.connect().await"
    )]
    pub fn connect_with_pipeline(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new_with_pipeline_and_poll_interval`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "tcp")]
    #[deprecated(
        note = "use AsyncTcpClient::new_with_pipeline_and_poll_interval(...) and then client.connect().await"
    )]
    pub fn connect_with_pipeline_and_poll_interval(
        host: &str,
        port: u16,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_pipeline_and_poll_interval(host, port, poll_interval)
    }

    /// Creates an async TCP client with compile-time pipeline depth `N`.
    ///
    /// Uses a 20 ms polling interval. Call [`AsyncClientCore::connect`] on the
    /// returned client before sending requests.
    #[cfg(feature = "tcp")]
    pub fn new_with_pipeline(host: &str, port: u16) -> Result<Self, AsyncError> {
        let transport = StdTcpTransport::new();
        let config = ModbusConfig::Tcp(ModbusTcpConfig::new(host, port)?);
        Self::from_transport_config(transport, config, Duration::from_millis(20))
    }

    /// Creates an async TCP client with compile-time pipeline depth `N` and a
    /// custom `poll_interval`.
    ///
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "tcp")]
    pub fn new_with_pipeline_and_poll_interval(
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
