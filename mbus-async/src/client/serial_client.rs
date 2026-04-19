//! Async Modbus serial client.
//!
//! [`AsyncSerialClient`] is a thin wrapper around [`AsyncClientCore`] that adds
//! serial-specific constructors (RTU, ASCII, and injection of a custom
//! transport).  All Modbus request methods are inherited transparently through
//! the [`std::ops::Deref`] implementation that resolves to `AsyncClientCore`.
//!
//! # Note on pipeline depth
//!
//! Serial Modbus is a strict request-reply protocol, so the background task is
//! always built with a pipeline depth of 1 (`ClientTask::<_, 1>`).

use std::ops::Deref;
use std::sync::Arc;
use std::time::Duration;

use mbus_core::errors::MbusError;
use mbus_core::transport::{AsyncTransport, ModbusConfig};
#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
use mbus_core::transport::{ModbusSerialConfig, SerialMode};
#[cfg(feature = "serial-ascii")]
use mbus_serial::TokioAsciiTransport;
#[cfg(feature = "serial-rtu")]
use mbus_serial::TokioRtuTransport;
use tokio::sync::{mpsc, watch};

use super::{AsyncClientCore, AsyncError};
use crate::client::task::{ClientTask, ConnectFactory};

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

// ── Constructors ──────────────────────────────────────────────────────────────

impl AsyncSerialClient {
    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_rtu`] and then call `client.connect().await?`.
    #[cfg(feature = "serial-rtu")]
    #[deprecated(note = "use AsyncSerialClient::new_rtu(...) and then client.connect().await")]
    pub fn connect_rtu(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        Self::new_rtu(serial_config)
    }

    /// Deprecated constructor alias.
    #[cfg(feature = "serial-rtu")]
    #[deprecated(note = "use AsyncSerialClient::new_rtu(...) and then client.connect().await")]
    pub fn connect_rtu_with_poll_interval(
        serial_config: ModbusSerialConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_rtu(serial_config)
    }

    /// Deprecated constructor alias.
    #[cfg(feature = "serial-ascii")]
    #[deprecated(note = "use AsyncSerialClient::new_ascii(...) and then client.connect().await")]
    pub fn connect_ascii(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        Self::new_ascii(serial_config)
    }

    /// Deprecated constructor alias.
    #[cfg(feature = "serial-ascii")]
    #[deprecated(note = "use AsyncSerialClient::new_ascii(...) and then client.connect().await")]
    pub fn connect_ascii_with_poll_interval(
        serial_config: ModbusSerialConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_ascii(serial_config)
    }

    /// Deprecated constructor alias.
    #[deprecated(
        note = "use AsyncSerialClient::new_with_transport(...) and then client.connect().await"
    )]
    pub fn connect_with_transport<T>(
        transport: T,
        config: ModbusConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError>
    where
        T: AsyncTransport + Send + 'static,
    {
        Self::new_with_transport(transport, config, poll_interval)
    }

    /// Creates an async Modbus RTU serial client without connecting.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Rtu`].
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "serial-rtu")]
    pub fn new_rtu(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Rtu {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }
        make_rtu_client(ModbusConfig::Serial(serial_config))
    }

    /// Creates an async Modbus RTU serial client with a custom `poll_interval`.
    ///
    /// The poll interval is ignored in the async implementation.
    #[cfg(feature = "serial-rtu")]
    pub fn new_rtu_with_poll_interval(
        serial_config: ModbusSerialConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_rtu(serial_config)
    }

    /// Creates an async Modbus ASCII serial client without connecting.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Ascii`].
    /// Call [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "serial-ascii")]
    pub fn new_ascii(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Ascii {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }
        make_ascii_client(ModbusConfig::Serial(serial_config))
    }

    /// Creates an async Modbus ASCII serial client with a custom `poll_interval`.
    ///
    /// The poll interval is ignored in the async implementation.
    #[cfg(feature = "serial-ascii")]
    pub fn new_ascii_with_poll_interval(
        serial_config: ModbusSerialConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_ascii(serial_config)
    }

    /// Creates an async serial client from a caller-provided transport without
    /// connecting.
    ///
    /// This is the escape hatch for custom serial drivers and integration tests
    /// that inject a mock transport.  The `config` must be
    /// `ModbusConfig::Serial(_)` or the call returns
    /// `AsyncError::Mbus(MbusError::InvalidTransport)`.  Call
    /// [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    pub fn new_with_transport<T>(
        transport: T,
        config: ModbusConfig,
        _poll_interval: Duration,
    ) -> Result<Self, AsyncError>
    where
        T: AsyncTransport + Send + 'static,
    {
        if !matches!(config, ModbusConfig::Serial(_)) {
            return Err(AsyncError::Mbus(MbusError::InvalidTransport));
        }

        // One-shot slot: the factory yields the transport on the first Connect,
        // then signals ConnectionClosed if reconnection is attempted.
        let slot = Arc::new(std::sync::Mutex::new(Some(transport)));
        let connect_fn: ConnectFactory<T> = Box::new(move || {
            let s = slot.clone();
            Box::pin(async move { s.lock().unwrap().take().ok_or(MbusError::ConnectionClosed) })
        });

        spawn_serial_task(connect_fn)
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Spawns a [`ClientTask`] with the given factory and returns an
/// [`AsyncSerialClient`] wired to it.
fn spawn_serial_task<T: AsyncTransport + Send + 'static>(
    connect_fn: ConnectFactory<T>,
) -> Result<AsyncSerialClient, AsyncError> {
    let handle = tokio::runtime::Handle::try_current().map_err(|_| AsyncError::WorkerClosed)?;
    let (cmd_tx, cmd_rx) = mpsc::channel(64);
    let (pending_count_tx, pending_count_rx) = watch::channel(0usize);

    #[cfg(feature = "traffic")]
    let notifier = crate::client::notifier::new_notifier_store();

    let task = ClientTask::<T, 1>::new(
        connect_fn,
        cmd_rx,
        pending_count_tx,
        #[cfg(feature = "traffic")]
        notifier.clone(),
    );
    handle.spawn(task.run());

    Ok(AsyncSerialClient {
        core: AsyncClientCore::new(
            cmd_tx,
            pending_count_rx,
            #[cfg(feature = "traffic")]
            notifier,
        ),
    })
}

/// Builds an RTU [`ConnectFactory`] that opens a fresh serial connection each
/// time it is called.
#[cfg(feature = "serial-rtu")]
fn make_rtu_factory(config: Arc<ModbusConfig>) -> ConnectFactory<TokioRtuTransport> {
    Box::new(move || {
        let cfg = config.clone();
        Box::pin(async move { TokioRtuTransport::new(&cfg) })
    })
}

/// Builds an ASCII [`ConnectFactory`] that opens a fresh serial connection each
/// time it is called.
#[cfg(feature = "serial-ascii")]
fn make_ascii_factory(config: Arc<ModbusConfig>) -> ConnectFactory<TokioAsciiTransport> {
    Box::new(move || {
        let cfg = config.clone();
        Box::pin(async move { TokioAsciiTransport::new(&cfg) })
    })
}

/// Creates a full [`AsyncSerialClient`] for RTU mode.
#[cfg(feature = "serial-rtu")]
fn make_rtu_client(config: ModbusConfig) -> Result<AsyncSerialClient, AsyncError> {
    spawn_serial_task(make_rtu_factory(Arc::new(config)))
}

/// Creates a full [`AsyncSerialClient`] for ASCII mode.
#[cfg(feature = "serial-ascii")]
fn make_ascii_client(config: ModbusConfig) -> Result<AsyncSerialClient, AsyncError> {
    spawn_serial_task(make_ascii_factory(Arc::new(config)))
}
