#[cfg(all(
    any(feature = "serial-client", feature = "serial-server"),
    not(target_arch = "wasm32")
))]
pub mod std_serial;

#[cfg(all(feature = "serial-async", not(target_arch = "wasm32")))]
pub mod async_serial;

#[cfg(all(feature = "serial-wasm", target_arch = "wasm32"))]
pub mod wasm_serial;

#[cfg(all(feature = "serial-wasm-async", target_arch = "wasm32"))]
pub mod wasm_async_serial;
