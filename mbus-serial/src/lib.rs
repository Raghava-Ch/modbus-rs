mod management;

#[cfg(all(
    any(feature = "serial-client", feature = "serial-server"),
    not(target_arch = "wasm32")
))]
pub use management::std_serial::*;

#[cfg(all(feature = "serial-async", not(target_arch = "wasm32")))]
pub use management::async_serial::{TokioAsciiTransport, TokioRtuTransport};

#[cfg(all(feature = "serial-wasm", target_arch = "wasm32"))]
pub use management::wasm_serial::*;

#[cfg(all(feature = "serial-wasm-async", target_arch = "wasm32"))]
pub use management::wasm_async_serial::{
    WasmAsyncAsciiTransport, WasmAsyncRtuTransport, WasmAsyncSerialTransport,
};
