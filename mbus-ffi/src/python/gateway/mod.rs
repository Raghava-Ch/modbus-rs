//! Python bindings for `mbus-gateway`.
//!
//! Exposes [`AsyncTcpGateway`](async_tcp::AsyncTcpGateway) (asyncio coroutine
//! API) and [`TcpGateway`](sync_tcp::TcpGateway) (blocking wrapper) backed by
//! [`mbus_gateway::AsyncTcpGatewayServer`].
//!
//!
//! The `GatewayEventHandler` Python class allows you to subclass and receive
//! telemetry events for session routing, forwarding, and errors.

pub mod async_tcp;
pub mod composite_router;
pub mod event_adapter;
pub mod event_handler;
pub mod sync_tcp;
