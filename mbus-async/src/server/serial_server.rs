//! [`AsyncSerialServer`] — async Modbus serial server (RTU and ASCII).

use mbus_core::transport::UnitIdOrSlaveAddr;

use super::app_handler::{AsyncAppHandler, AsyncServerError};
use super::session::AsyncServerSession;

/// Async Modbus serial server.
///
/// Unlike TCP, a serial bus is always single-connection: one frame in, one frame out.
/// This struct wraps an already-opened transport and exposes the session loop via
/// [`run`](AsyncSerialServer::run).
///
/// # Usage
///
/// ```rust,ignore
/// let mut server = AsyncSerialServer::new_rtu(&config).await?;
/// server.run(MyApp::default()).await?;
/// ```
#[cfg(feature = "server-serial")]
pub struct AsyncSerialServer<T: mbus_core::transport::AsyncTransport + Send> {
    session: AsyncServerSession<T>,
}

#[cfg(feature = "server-serial")]
impl<T: mbus_core::transport::AsyncTransport + Send> AsyncSerialServer<T> {
    /// Run the server loop until the port is closed.
    ///
    /// Calls `session.run(app)` internally.
    pub async fn run<APP: AsyncAppHandler>(
        &mut self,
        mut app: APP,
    ) -> Result<(), AsyncServerError> {
        self.session.run(&mut app).await
    }

    // ── Constructors ─────────────────────────────────────────────────────────

    /// Construct from any transport that implements [`AsyncTransport`].
    pub fn from_transport(transport: T, unit: UnitIdOrSlaveAddr) -> Self {
        Self {
            session: AsyncServerSession::new(transport, unit),
        }
    }
}

// ── RTU / ASCII convenience constructors ─────────────────────────────────────

/// Type alias for an RTU serial server.
#[cfg(feature = "server-serial")]
pub type AsyncRtuServer = AsyncSerialServer<mbus_serial::TokioRtuTransport>;

/// Type alias for an ASCII serial server.
#[cfg(feature = "server-serial")]
pub type AsyncAsciiServer = AsyncSerialServer<mbus_serial::TokioAsciiTransport>;

#[cfg(feature = "server-serial")]
impl AsyncRtuServer {
    /// Construct an RTU server over the port described by `config`.
    pub fn new_rtu(
        config: &mbus_core::transport::ModbusConfig,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<Self, AsyncServerError> {
        let transport =
            mbus_serial::TokioRtuTransport::new(config).map_err(AsyncServerError::Transport)?;
        Ok(Self::from_transport(transport, unit))
    }
}

#[cfg(feature = "server-serial")]
impl AsyncAsciiServer {
    /// Construct an ASCII server over the port described by `config`.
    pub fn new_ascii(
        config: &mbus_core::transport::ModbusConfig,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<Self, AsyncServerError> {
        let transport =
            mbus_serial::TokioAsciiTransport::new(config).map_err(AsyncServerError::Transport)?;
        Ok(Self::from_transport(transport, unit))
    }
}
