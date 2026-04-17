mod management;

#[cfg(not(target_arch = "wasm32"))]
pub use management::std_serial::*;

#[cfg(all(feature = "async", not(target_arch = "wasm32")))]
pub use management::async_serial::{TokioRtuTransport, TokioAsciiTransport};

#[cfg(all(feature = "wasm", target_arch = "wasm32"))]
pub use management::wasm_serial::*;
