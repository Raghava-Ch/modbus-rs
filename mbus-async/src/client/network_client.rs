//! Async Modbus TCP client.
//!
//! [`AsyncTcpClient`] is a thin wrapper around [`AsyncClientCore`] that adds
//! TCP-specific constructors. All Modbus request methods are inherited
//! transparently through the [`std::ops::Deref`] implementation that resolves
//! to `AsyncClientCore`.

use std::ops::Deref;
use std::time::Duration;

#[cfg(feature = "network-tcp")]
use mbus_core::transport::ModbusTcpConfig;
#[cfg(feature = "network-tcp")]
use mbus_network::TokioTcpTransport;
use tokio::sync::{mpsc, watch};

use super::{AsyncClientCore, AsyncError};
use crate::client::task::{ClientTask, ConnectFactory};

/// Async Modbus TCP client facade.
///
/// All Modbus request methods (`read_holding_registers`, `write_single_coil`,
/// etc.) are available directly on this type via [`Deref`] to
/// [`AsyncClientCore`].
///
/// The constant generic parameter `N` is the compile-time pipeline depth
/// (default `9`).
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
    #[cfg(feature = "network-tcp")]
    #[deprecated(note = "use AsyncTcpClient::new(...) and then client.connect().await")]
    pub fn connect(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new(host, port)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new`] and then call `client.connect().await?`.
    #[cfg(feature = "network-tcp")]
    #[deprecated(note = "use AsyncTcpClient::new(...) and then client.connect().await")]
    pub fn connect_with_poll_interval(
        host: &str,
        port: u16,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new(host, port)
    }

    /// Creates an async TCP client for `host`:`port` without connecting.
    ///
    /// Uses the default pipeline depth of 9. Call [`AsyncClientCore::connect`]
    /// on the returned client before sending requests.
    #[cfg(feature = "network-tcp")]
    pub fn new(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Creates an async TCP client for `host`:`port` with a custom
    /// `poll_interval`.
    ///
    /// The poll interval is ignored in the async implementation.
    /// Uses the default pipeline depth of 9. Call [`AsyncClientCore::connect`]
    /// on the returned client before sending requests.
    #[cfg(feature = "network-tcp")]
    pub fn new_with_poll_interval(
        host: &str,
        port: u16,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new(host, port)
    }

    /// Creates an async TCP client with a fully custom [`ModbusTcpConfig`],
    /// using the default pipeline depth of 9.
    ///
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "network-tcp")]
    pub fn new_with_config(
        tcp_config: ModbusTcpConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::from_connect_fn(make_tcp_factory(
            tcp_config.host.as_str().to_string(),
            tcp_config.port,
        ))
    }
}

// ── Configurable-pipeline constructors ───────────────────────────────────────

impl<const N: usize> AsyncTcpClient<N> {
    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new_with_pipeline`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "network-tcp")]
    #[deprecated(
        note = "use AsyncTcpClient::new_with_pipeline(...) and then client.connect().await"
    )]
    pub fn connect_with_pipeline(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncTcpClient::new_with_pipeline_and_poll_interval`] and then
    /// call `client.connect().await?`.
    #[cfg(feature = "network-tcp")]
    #[deprecated(
        note = "use AsyncTcpClient::new_with_pipeline_and_poll_interval(...) and then client.connect().await"
    )]
    pub fn connect_with_pipeline_and_poll_interval(
        host: &str,
        port: u16,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Creates an async TCP client with compile-time pipeline depth `N`.
    ///
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "network-tcp")]
    pub fn new_with_pipeline(host: &str, port: u16) -> Result<Self, AsyncError> {
        Self::from_connect_fn(make_tcp_factory(host.to_string(), port))
    }

    /// Creates an async TCP client with compile-time pipeline depth `N` and a
    /// custom `poll_interval`.
    ///
    /// The poll interval is ignored in the async implementation.
    ///
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "network-tcp")]
    pub fn new_with_pipeline_and_poll_interval(
        host: &str,
        port: u16,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_pipeline(host, port)
    }

    /// Creates an async TCP client with a fully custom config and pipeline
    /// depth `N`.
    ///
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "network-tcp")]
    pub fn new_with_config_and_pipeline(
        tcp_config: ModbusTcpConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::from_connect_fn(make_tcp_factory(
            tcp_config.host.as_str().to_string(),
            tcp_config.port,
        ))
    }

    /// Internal constructor: wires a `ConnectFactory` into a spawned
    /// [`ClientTask`] and wraps the resulting channels in an
    /// [`AsyncClientCore`].
    #[cfg(feature = "network-tcp")]
    fn from_connect_fn(connect_fn: ConnectFactory<TokioTcpTransport>) -> Result<Self, AsyncError> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| AsyncError::WorkerClosed)?;
        let (cmd_tx, cmd_rx) = mpsc::channel(64);
        let (pending_count_tx, pending_count_rx) = watch::channel(0usize);

        #[cfg(feature = "traffic")]
        let notifier = crate::client::notifier::new_notifier_store();

        let task = ClientTask::<TokioTcpTransport, N>::new(
            connect_fn,
            cmd_rx,
            pending_count_tx,
            #[cfg(feature = "traffic")]
            notifier.clone(),
        );
        handle.spawn(task.run());

        Ok(Self {
            core: AsyncClientCore::new(
                cmd_tx,
                pending_count_rx,
                #[cfg(feature = "traffic")]
                notifier,
            ),
        })
    }
}

// ── Internal helpers ─────────────────────────────────────────────────────────

/// Builds a [`ConnectFactory`] that resolves a TCP connection to `host:port`.
#[cfg(feature = "network-tcp")]
fn make_tcp_factory(host: String, port: u16) -> ConnectFactory<TokioTcpTransport> {
    Box::new(move || {
        let h = host.clone();
        Box::pin(async move { TokioTcpTransport::connect((h.as_str(), port)).await })
    })
}
