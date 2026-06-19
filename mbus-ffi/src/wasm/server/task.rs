//! `WasmServerTask` — event loop driving the `AsyncServerSession` for WASM servers.

use futures_channel::oneshot;
use futures_util::FutureExt;
use mbus_async::server::session::AsyncServerSession;
use mbus_core::transport::UnitIdOrSlaveAddr;

use super::handlers::JsServerHandlers;

pub struct WasmServerTask<T> {
    transport: T,
    handlers: JsServerHandlers,
    unit: UnitIdOrSlaveAddr,
    shutdown_rx: oneshot::Receiver<()>,
}

impl<T: mbus_core::transport::AsyncTransport + Send + 'static> WasmServerTask<T> {
    /// Creates a new `WasmServerTask` with the given transport, handler callbacks,
    /// unit ID, and a shutdown channel receiver.
    pub fn new(
        transport: T,
        handlers: JsServerHandlers,
        unit: UnitIdOrSlaveAddr,
        shutdown_rx: oneshot::Receiver<()>,
    ) -> Self {
        Self {
            transport,
            handlers,
            unit,
            shutdown_rx,
        }
    }

    /// Runs the server loop until connection is lost or shutdown is signaled.
    pub async fn run(self) -> Result<(), wasm_bindgen::JsValue> {
        let mut session = AsyncServerSession::new(self.transport, self.unit);
        session.set_broadcast_writes(T::SUPPORTS_BROADCAST_WRITES);

        let mut handlers = self.handlers;

        futures_util::select! {
            _ = self.shutdown_rx.fuse() => {
                Ok(())
            },
            res = session.run(&mut handlers).fuse() => {
                match res {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        let err_msg = format!("Server session run failed: {:?}", e);
                        web_sys::console::error_1(&wasm_bindgen::JsValue::from_str(&format!("[Rust ServerTask] {}", err_msg)));
                        Err(wasm_bindgen::JsValue::from_str(&err_msg))
                    }
                }
            },
        }
    }
}
