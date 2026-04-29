mod management;

#[cfg(not(target_arch = "wasm32"))]
pub use management::std_transport::*;

#[cfg(not(target_arch = "wasm32"))]
pub use management::server_transport::*;

#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
pub use management::async_transport::TokioTcpTransport;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub use management::wasm_transport::WasmWsTransport;

/// Server-side WebSocket upstream transport (enabled by the `ws-server` feature).
///
/// `WsUpstreamTransport` wraps an accepted `tokio-tungstenite` stream and
/// implements `AsyncTransport`, allowing it to be plugged into
/// `AsyncWsGatewayServer` (or any generic async session loop) as the upstream
/// side of a WASM → raw-TCP gateway.
#[cfg(all(feature = "ws-server", not(target_arch = "wasm32")))]
pub use management::ws_upstream_transport::WsUpstreamTransport;
