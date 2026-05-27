//! Graceful shutdown primitives for async gateway servers.
//!
//! All async gateway servers — [`AsyncTcpGatewayServer`], [`AsyncWsGatewayServer`],
//! [`AsyncSerialGatewayServer`], and [`AsyncRawGatewayServer`] — expose a
//! `serve_with_shutdown` method that accepts any `Future<Output = ()>` as a
//! shutdown signal.
//!
//! [`GatewayShutdown`] provides a ergonomic cancellation-token-style API built on
//! a plain `tokio::sync::oneshot` channel — **no external crate required**.
//!
//! ## Quick start
//!
//! ```rust
//! use mbus_gateway::GatewayShutdown;
//!
//! # tokio_test::block_on(async {
//! let (token, shutdown) = GatewayShutdown::new();
//!
//! // From any task or signal handler:
//! token.cancel();
//!
//! // Pass `shutdown` to any `serve_with_shutdown`:
//! // AsyncTcpGatewayServer::serve_with_shutdown("0.0.0.0:502", router, ds, shutdown).await?;
//! // (here we just await it to demonstrate)
//! shutdown.await;
//! # });
//! ```
//!
//! ## Signal integration
//!
//! ```rust,no_run
//! use mbus_gateway::{GatewayShutdown, NoopEventHandler};
//! use mbus_gateway::AsyncTcpGatewayServer;
//! use mbus_gateway::UnitRouteTable;
//! use mbus_network::TokioTcpTransport;
//! use std::sync::Arc;
//! use std::time::Duration;
//! use tokio::sync::Mutex;
//!
//! # async fn example() {
//! let (token, shutdown) = GatewayShutdown::new();
//!
//! // Spawn a signal handler in a background task.
//! tokio::spawn(async move {
//!     tokio::signal::ctrl_c().await.expect("ctrl-c handler failed");
//!     println!("shutdown signal received");
//!     token.cancel();
//! });
//!
//! let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
//! let router = UnitRouteTable::<4>::new();
//!
//! let handler = Arc::new(Mutex::new(NoopEventHandler));
//! AsyncTcpGatewayServer::serve_with_shutdown(
//!     "0.0.0.0:502",
//!     router,
//!     vec![Arc::new(Mutex::new(downstream))],
//!     handler,
//!     Duration::from_secs(1),
//!     shutdown,
//! ).await.unwrap();
//! # }
//! ```
//!
//! ## Using with `tokio::select!` directly
//!
//! If you prefer raw futures, every `serve_with_shutdown` accepts **any**
//! `Future<Output = ()>`, so you can pass arbitrary futures directly:
//!
//! ```rust,no_run
//! # async fn example() {
//! # use mbus_gateway::{AsyncTcpGatewayServer, UnitRouteTable, NoopEventHandler};
//! # use mbus_network::TokioTcpTransport;
//! # use std::sync::Arc;
//! # use std::time::Duration;
//! # use tokio::sync::Mutex;
//! let downstream = TokioTcpTransport::connect("192.168.1.10:502").await.unwrap();
//! let router = UnitRouteTable::<4>::new();
//!
//! // Shutdown after 60 s.
//! let shutdown = tokio::time::sleep(std::time::Duration::from_secs(60));
//!
//! let handler = Arc::new(Mutex::new(NoopEventHandler));
//! AsyncTcpGatewayServer::serve_with_shutdown(
//!     "0.0.0.0:502",
//!     router,
//!     vec![Arc::new(Mutex::new(downstream))],
//!     handler,
//!     Duration::from_secs(1),
//!     shutdown,
//! ).await.unwrap();
//! # }
//! ```
//!
//! [`AsyncTcpGatewayServer`]: crate::AsyncTcpGatewayServer
//! [`AsyncWsGatewayServer`]: crate::AsyncWsGatewayServer
//! [`AsyncSerialGatewayServer`]: crate::AsyncSerialGatewayServer
//! [`AsyncRawGatewayServer`]: crate::AsyncRawGatewayServer

use tokio::sync::oneshot;

// ─────────────────────────────────────────────────────────────────────────────
// GatewayShutdownToken
// ─────────────────────────────────────────────────────────────────────────────

/// The send half of a [`GatewayShutdown`] pair.
///
/// Call [`cancel()`](Self::cancel) to fire the shutdown signal.  Safe to clone
/// and share across tasks — only one cancel call is needed.
#[derive(Clone, Debug)]
pub struct GatewayShutdownToken {
    tx: std::sync::Arc<tokio::sync::Mutex<Option<oneshot::Sender<()>>>>,
}

impl GatewayShutdownToken {
    /// Signal all listeners created from the paired [`GatewayShutdown`] to shut down.
    ///
    /// Idempotent: calling `cancel()` more than once has no effect.
    pub fn cancel(&self) {
        // Grab the sender from the Option and send.  Subsequent calls find None.
        if let Ok(mut guard) = self.tx.try_lock()
            && let Some(tx) = guard.take()
        {
            let _ = tx.send(());
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// GatewayShutdown
// ─────────────────────────────────────────────────────────────────────────────

/// A `Future<Output = ()>` that resolves when the paired [`GatewayShutdownToken`]
/// is cancelled.
///
/// Create a pair with [`GatewayShutdown::new()`] and pass the `GatewayShutdown`
/// to any `serve_with_shutdown` method:
///
/// ```rust
/// use mbus_gateway::GatewayShutdown;
///
/// # tokio_test::block_on(async {
/// let (token, shutdown) = GatewayShutdown::new();
/// token.cancel();
/// shutdown.await; // resolves immediately
/// # });
/// ```
#[must_use = "GatewayShutdown does nothing unless awaited"]
pub struct GatewayShutdown {
    rx: oneshot::Receiver<()>,
}

impl GatewayShutdown {
    /// Create a linked [`GatewayShutdownToken`] / [`GatewayShutdown`] pair.
    pub fn new() -> (GatewayShutdownToken, Self) {
        let (tx, rx) = oneshot::channel();
        let token = GatewayShutdownToken {
            tx: std::sync::Arc::new(tokio::sync::Mutex::new(Some(tx))),
        };
        (token, Self { rx })
    }
}

impl std::future::Future for GatewayShutdown {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        // If the sender was dropped or sent, resolve to ().
        match std::pin::Pin::new(&mut self.rx).poll(cx) {
            std::task::Poll::Ready(_) => std::task::Poll::Ready(()),
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}
