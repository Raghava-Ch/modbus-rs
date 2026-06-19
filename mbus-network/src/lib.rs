mod management;

#[cfg(all(feature = "tcp-client", not(target_arch = "wasm32")))]
pub use management::std_transport::*;

#[cfg(all(feature = "tcp-server", not(target_arch = "wasm32")))]
pub use management::server_transport::*;

#[cfg(all(feature = "tcp-async", not(target_arch = "wasm32")))]
pub use management::async_transport::TokioTcpTransport;

#[cfg(all(feature = "ws-client", target_arch = "wasm32"))]
pub use management::wasm_transport::WasmWsTransport;

#[cfg(all(feature = "ws-client", target_arch = "wasm32"))]
pub use management::wasm_async_transport::WasmAsyncTransport;

/// Server-side WebSocket upstream transport (enabled by the `ws-server` feature).
///
/// `WsUpstreamTransport` wraps an accepted `tokio-tungstenite` stream and
/// implements `AsyncTransport`, allowing it to be plugged into
/// `AsyncWsGatewayServer` (or any generic async session loop) as the upstream
/// side of a WASM → raw-TCP gateway.
#[cfg(all(feature = "ws-server", not(target_arch = "wasm32")))]
pub use management::ws_upstream_transport::WsUpstreamTransport;
