#[cfg(not(target_arch = "wasm32"))]
pub mod std_serial;

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub mod wasm_serial;
