//! Async TCP-gateway server bindings.

use futures_channel::oneshot;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::WasmAsyncTransport;
use wasm_bindgen::prelude::*;

use super::binding_types::WasmTcpServerOptions;
use super::handlers::JsServerHandlers;
use super::task::WasmServerTask;
use crate::wasm::client::helpers::{get_string, get_u8};

use std::future::Future;
use std::pin::Pin;

use std::sync::Mutex;

#[wasm_bindgen]
/// Browser-facing Modbus TCP server proxy running over a WebSocket gateway.
pub struct WasmTcpServer {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    task_fut: Mutex<Option<Pin<Box<dyn Future<Output = Result<(), JsValue>> + Send>>>>,
}

#[wasm_bindgen]
impl WasmTcpServer {
    /// Binds to a WebSocket gateway URL.
    ///
    /// The `options` contains `wsUrl` and `unitId`.
    /// The `handlers` is the JS callback registry.
    pub async fn bind(
        options: WasmTcpServerOptions,
        handlers: JsValue,
    ) -> Result<WasmTcpServer, JsValue> {
        let options_val = JsValue::from(options);
        let ws_url = get_string(&options_val, "wsUrl", "");
        if ws_url.is_empty() {
            return Err(JsValue::from_str("Missing or empty 'wsUrl'"));
        }
        let unit_id = get_u8(&options_val, "unitId", 1);
        let unit =
            UnitIdOrSlaveAddr::new(unit_id).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        // 1. Connect websocket transport
        let transport = WasmAsyncTransport::connect(&ws_url).await?;

        // 2. Setup channels
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // 3. Create server task
        let handlers = JsServerHandlers::new(handlers);
        let task = WasmServerTask::new(transport, handlers, unit, shutdown_rx);
        let task_fut = Box::pin(task.run());

        Ok(WasmTcpServer {
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            task_fut: Mutex::new(Some(task_fut)),
        })
    }

    /// Runs the server loop. Returns a promise that resolves on clean shutdown
    /// or rejects with the error that caused the server to stop.
    pub async fn serve(&self) -> Result<(), JsValue> {
        let fut = self
            .task_fut
            .lock()
            .unwrap()
            .take()
            .ok_or_else(|| JsValue::from_str("Server is already running or shut down"))?;
        fut.await
    }

    /// Shutdown the server.
    pub async fn shutdown(&self) -> Result<(), JsValue> {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}
