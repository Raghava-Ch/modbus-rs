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

use super::*;
use std::ops::Deref;

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
    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_rtu`] and then call `client.connect().await?`.
    #[cfg(feature = "serial-rtu")]
    #[deprecated(note = "use AsyncSerialClient::new_rtu(...) and then client.connect().await")]
    pub fn connect_rtu(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        Self::new_rtu(serial_config)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_rtu_with_poll_interval`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "serial-rtu")]
    #[deprecated(
        note = "use AsyncSerialClient::new_rtu_with_poll_interval(...) and then client.connect().await"
    )]
    pub fn connect_rtu_with_poll_interval(
        serial_config: ModbusSerialConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_rtu_with_poll_interval(serial_config, poll_interval)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_ascii`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "serial-ascii")]
    #[deprecated(note = "use AsyncSerialClient::new_ascii(...) and then client.connect().await")]
    pub fn connect_ascii(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        Self::new_ascii(serial_config)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_ascii_with_poll_interval`] and then call
    /// `client.connect().await?`.
    #[cfg(feature = "serial-ascii")]
    #[deprecated(
        note = "use AsyncSerialClient::new_ascii_with_poll_interval(...) and then client.connect().await"
    )]
    pub fn connect_ascii_with_poll_interval(
        serial_config: ModbusSerialConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_ascii_with_poll_interval(serial_config, poll_interval)
    }

    /// Deprecated constructor alias.
    ///
    /// Use [`AsyncSerialClient::new_with_transport`] and then call
    /// `client.connect().await?`.
    #[deprecated(
        note = "use AsyncSerialClient::new_with_transport(...) and then client.connect().await"
    )]
    pub fn connect_with_transport<TRANSPORT>(
        transport: TRANSPORT,
        config: ModbusConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError>
    where
        TRANSPORT: Transport + Send + 'static,
    {
        Self::new_with_transport(transport, config, poll_interval)
    }

    /// Creates an async Modbus RTU serial client without connecting.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Rtu`]. Uses a
    /// 20 ms polling interval. Call [`AsyncClientCore::connect`] on the returned
    /// client before sending requests.
    #[cfg(feature = "serial-rtu")]
    pub fn new_rtu(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Rtu {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }

        let transport = StdRtuTransport::new();
        let config = ModbusConfig::Serial(serial_config);
        Self::from_transport_config(transport, config, Duration::from_millis(20))
    }

    /// Creates an async Modbus RTU serial client with a custom `poll_interval`.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Rtu`]. Call
    /// [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "serial-rtu")]
    pub fn new_rtu_with_poll_interval(
        serial_config: ModbusSerialConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Rtu {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }

        let transport = StdRtuTransport::new();
        let config = ModbusConfig::Serial(serial_config);
        Self::from_transport_config(transport, config, poll_interval)
    }

    /// Creates an async Modbus ASCII serial client without connecting.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Ascii`]. Uses a
    /// 20 ms polling interval. Call [`AsyncClientCore::connect`] on the returned
    /// client before sending requests.
    #[cfg(feature = "serial-ascii")]
    pub fn new_ascii(serial_config: ModbusSerialConfig) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Ascii {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }

        let transport = StdAsciiTransport::new();
        let config = ModbusConfig::Serial(serial_config);
        Self::from_transport_config(transport, config, Duration::from_millis(20))
    }

    /// Creates an async Modbus ASCII serial client with a custom `poll_interval`.
    ///
    /// Validates that `serial_config.mode` is [`SerialMode::Ascii`]. Call
    /// [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    #[cfg(feature = "serial-ascii")]
    pub fn new_ascii_with_poll_interval(
        serial_config: ModbusSerialConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        if serial_config.mode != SerialMode::Ascii {
            return Err(AsyncError::Mbus(MbusError::InvalidConfiguration));
        }

        let transport = StdAsciiTransport::new();
        let config = ModbusConfig::Serial(serial_config);
        Self::from_transport_config(transport, config, poll_interval)
    }

    /// Creates an async serial client from a caller-provided transport without
    /// connecting.
    ///
    /// This is the escape hatch for custom serial drivers and integration tests
    /// that inject a mock transport.  The `config` must be
    /// `ModbusConfig::Serial(_)` or the call returns
    /// `AsyncError::Mbus(MbusError::InvalidTransport)`. Call
    /// [`AsyncClientCore::connect`] on the returned client before sending
    /// requests.
    pub fn new_with_transport<TRANSPORT>(
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
        #[cfg(feature = "traffic")]
        let (traffic_watch, traffic_rx) = watch::channel(None);
        let app = AsyncApp {
            pending: pending.clone(),
            #[cfg(feature = "traffic")]
            traffic_watch,
        };

        // Serial is always single-in-flight (pipeline depth 1).
        let client = ClientServices::<_, _, 1>::new(transport, app, config)?;
        let (sender, receiver) = mpsc::channel();

        thread::spawn(move || run_worker(client, pending, receiver, poll_interval));

        #[cfg(feature = "traffic")]
        {
            Ok(Self {
                core: AsyncClientCore::new(sender, traffic_rx),
            })
        }

        #[cfg(not(feature = "traffic"))]
        {
            Ok(Self {
                core: AsyncClientCore::new(sender),
            })
        }
    }

    /// Internal constructor used by the RTU/ASCII helpers.
    #[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
    fn from_transport_config<const ASCII: bool>(
        transport: StdSerialTransport<ASCII>,
        config: ModbusConfig,
        poll_interval: Duration,
    ) -> Result<Self, AsyncError> {
        Self::new_with_transport(transport, config, poll_interval)
    }
}
