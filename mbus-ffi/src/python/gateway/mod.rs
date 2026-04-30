//! Python bindings for `mbus-gateway`.
//!
//! Exposes [`AsyncTcpGateway`](async_tcp::AsyncTcpGateway) (asyncio coroutine
//! API) and [`TcpGateway`](sync_tcp::TcpGateway) (blocking wrapper) backed by
//! [`mbus_gateway::AsyncTcpGatewayServer`].
//!
//! The async server in `mbus-gateway` does not currently expose a per-session
//! event handler hook, so the Python bindings do not yet accept one. The
//! `GatewayEventHandler` Python class is provided as a forward-compatible
//! placeholder; instances are accepted by the constructors but ignored.

pub mod async_tcp;
pub mod composite_router;
pub mod event_handler;
pub mod sync_tcp;
