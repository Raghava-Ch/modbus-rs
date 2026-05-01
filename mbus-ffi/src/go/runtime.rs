//! Shared Tokio runtime for every `mbus_go_*` entry point.
//!
//! Initialised on first call; the same runtime is reused by every handle so
//! we never spin up redundant OS threads.

use std::sync::OnceLock;
use tokio::runtime::Runtime;

static TOKIO_RT: OnceLock<Runtime> = OnceLock::new();

/// Returns a reference to the module-wide Tokio runtime.
pub fn get() -> &'static Runtime {
    TOKIO_RT.get_or_init(|| {
        Runtime::new().expect("failed to create Tokio runtime for modbus_rs Go bindings")
    })
}
