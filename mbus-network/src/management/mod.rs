#[cfg(all(feature = "tcp-client", not(target_arch = "wasm32")))]
pub mod std_transport;

#[cfg(all(feature = "tcp-server", not(target_arch = "wasm32")))]
pub mod server_transport;

#[cfg(all(feature = "tcp-async", not(target_arch = "wasm32")))]
pub mod async_transport;

#[cfg(all(feature = "ws-client", target_arch = "wasm32"))]
pub mod wasm_transport;

#[cfg(all(feature = "ws-client", target_arch = "wasm32"))]
pub mod wasm_async_transport;

#[cfg(all(feature = "ws-server", not(target_arch = "wasm32")))]
pub mod ws_upstream_transport;
