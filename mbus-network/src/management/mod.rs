#[cfg(not(target_arch = "wasm32"))]
pub mod std_transport;

#[cfg(not(target_arch = "wasm32"))]
pub mod server_transport;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm_transport;
