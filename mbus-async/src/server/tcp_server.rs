//! [`AsyncTcpServer`] — multi-connection Modbus TCP server.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::Mutex;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;

use super::app_handler::{AsyncAppHandler, AsyncServerError};
use super::session::AsyncServerSession;

/// Async Modbus TCP server.
///
/// Binds to a TCP address and accepts multiple simultaneous client connections,
/// spawning an independent tokio task per session.
///
/// # Usage
///
/// **Level 1** — zero-boilerplate app with `#[async_modbus_app]`:
///
/// ```rust,ignore
/// AsyncTcpServer::serve("0.0.0.0:502", HvacApp::default(), unit_id(1)).await?;
/// ```
///
/// **Level 1 with shared state** (multiple clients share one `Arc<Mutex<APP>>`):
///
/// ```rust,ignore
/// let shared = Arc::new(Mutex::new(HvacApp::default()));
/// AsyncTcpServer::serve("0.0.0.0:502", shared, unit_id(1)).await?;
/// ```
pub struct AsyncTcpServer {
    listener: TcpListener,
    unit: UnitIdOrSlaveAddr,
}

impl AsyncTcpServer {
    // ── Serve ────────────────────────────────────────────────────────────────

    /// Bind and serve, running forever.
    ///
    /// Each accepted connection is handled in a dedicated `tokio::spawn`-ed task.
    /// `app` must implement [`AsyncAppHandler`] + `Clone` so each task gets its
    /// own instance; for shared state use `Arc<Mutex<APP>>` which also implements
    /// `AsyncAppHandler` via the blanket impl.
    ///
    /// Returns `Err(AsyncServerError::ConnectionClosed)` only if binding fails;
    /// individual session errors are silently dropped.
    pub async fn serve<APP, A>(
        addr: A,
        app: APP,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<Infallible, AsyncServerError>
    where
        A: ToSocketAddrs,
        APP: AsyncAppHandler + Clone,
    {
        let server = Self::bind(addr, unit).await?;
        loop {
            let (mut session, _peer) = server.accept().await?;
            let app_instance = app.clone();
            tokio::spawn(async move {
                let mut app_instance = app_instance;
                let _ = session.run(&mut app_instance).await;
            });
        }
    }

    /// Convenience constructor: wrap `Arc<Mutex<APP>>` automatically.
    ///
    /// All accepted sessions will share the same `APP` instance behind the mutex.
    pub async fn serve_shared<APP, A>(
        addr: A,
        app: Arc<Mutex<APP>>,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<Infallible, AsyncServerError>
    where
        A: ToSocketAddrs,
        APP: AsyncAppHandler,
    {
        Self::serve(addr, app, unit).await
    }

    // ── Advanced ─────────────────────────────────────────────────────────────

    /// Bind the server to `addr` and return a handle for a custom accept loop.
    pub async fn bind<A: ToSocketAddrs>(
        addr: A,
        unit: UnitIdOrSlaveAddr,
    ) -> Result<Self, AsyncServerError> {
        let listener = TcpListener::bind(addr)
            .await
            .map_err(AsyncServerError::BindFailed)?;
        Ok(Self { listener, unit })
    }

    /// Accept the next incoming connection, returning a session and the peer address.
    pub async fn accept(
        &self,
    ) -> Result<(AsyncServerSession<TokioTcpTransport>, SocketAddr), AsyncServerError> {
        let (stream, peer) = self
            .listener
            .accept()
            .await
            .map_err(|_| AsyncServerError::ConnectionClosed)?;
        let _ = stream.set_nodelay(true);
        let transport = TokioTcpTransport::from_stream(stream);
        let session = AsyncServerSession::new(transport, self.unit);
        Ok((session, peer))
    }

    /// The local address the server is listening on.
    ///
    /// Useful for integration tests that bind to port 0 and then need to know
    /// which port was assigned.
    pub fn local_addr(&self) -> Result<SocketAddr, AsyncServerError> {
        self.listener
            .local_addr()
            .map_err(|_| AsyncServerError::Transport(mbus_core::errors::MbusError::IoError))
    }
}
