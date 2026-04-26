//! [`AsyncTcpServer`] — multi-connection Modbus TCP server.

use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::TokioTcpTransport;
use std::convert::Infallible;
use std::future::Future;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::{TcpListener, ToSocketAddrs};
use tokio::sync::Mutex;

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

    /// Bind and serve until the `shutdown` future resolves.
    ///
    /// Identical to [`serve`](Self::serve) but accepts a future that, when it
    /// resolves, causes the accept loop to stop and this method to return
    /// `Ok(())` rather than `Err(Infallible)`.
    ///
    /// In-flight sessions are **not** cancelled; they run to completion in
    /// their own tokio tasks.  Only new connections are no longer accepted
    /// after the shutdown signal fires.
    ///
    /// ```rust,ignore
    /// let notify = Arc::new(tokio::sync::Notify::new());
    /// let n = notify.clone();
    /// AsyncTcpServer::serve_with_shutdown(
    ///     "0.0.0.0:502",
    ///     HvacApp::default(),
    ///     unit_id(1),
    ///     n.notified(),
    /// ).await?;
    /// ```
    pub async fn serve_with_shutdown<APP, A, F>(
        addr: A,
        app: APP,
        unit: UnitIdOrSlaveAddr,
        shutdown: F,
    ) -> Result<(), AsyncServerError>
    where
        A: ToSocketAddrs,
        APP: AsyncAppHandler + Clone,
        F: Future<Output = ()>,
    {
        let server = Self::bind(addr, unit).await?;
        tokio::pin!(shutdown);
        loop {
            tokio::select! {
                biased;
                _ = &mut shutdown => return Ok(()),
                result = server.accept() => {
                    let (mut session, _peer) = result?;
                    let app_instance = app.clone();
                    tokio::spawn(async move {
                        let mut app_instance = app_instance;
                        let _ = session.run(&mut app_instance).await;
                    });
                }
            }
        }
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
