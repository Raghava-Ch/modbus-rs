//! Async TCP-gateway server bindings.

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface WasmTcpServerOptions {
  wsUrl: string;
  unitId: number;
}
"#;

use futures_channel::oneshot;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_network::WasmAsyncTransport;
use wasm_bindgen::prelude::*;

use super::binding_types::WasmTcpServerOptions;
use super::handlers::JsServerHandlers;
use super::task::WasmServerTask;
use crate::wasm::client::helpers::{get_string, get_u8};


use std::sync::Mutex;

#[wasm_bindgen]
/// A browser-facing Modbus server that communicates over a WebSocket gateway.
///
/// This class allows you to create a simulated Modbus TCP device that can be accessed
/// by other applications through a WebSocket-to-TCP proxy, such as the `modbus-gateway` application.
/// An instance is created via the static `bind` method.
/// Browser-facing Modbus TCP server proxy running over a WebSocket gateway.
pub struct WasmTcpServer {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>, // Sender to signal the server task to shut down.
    task_fut: Mutex<Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), JsValue>> + Send>>>>, // The future representing the running server task.
}

#[wasm_bindgen]
impl WasmTcpServer {
    /// Creates and binds a new Modbus TCP server to a WebSocket gateway.
    ///
    /// @param {WasmTcpServerOptions} options - The server configuration.
    /// @param {string} options.wsUrl - The URL of the WebSocket gateway that will proxy TCP traffic.
    /// @param {number} options.unitId - The Modbus unit ID (1-247) the server will respond to.
    /// @param {object} handlers - An object containing callback functions to handle incoming Modbus requests (e.g., `onReadHoldingRegisters`).
    /// @returns {Promise<WasmTcpServer>} A promise that resolves to a new `WasmTcpServer` instance.
    ///
    /// @example
    /// const handlers = {
    ///   onReadHoldingRegisters: (req) => [10, 20, 30]
    /// };
    /// const server = await WasmTcpServer.bind({ wsUrl: "ws://localhost:8080", unitId: 1 }, handlers);
    /// console.log("Server bound!");
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

    /// Starts the server's event loop to listen for and process incoming requests.
    ///
    /// This method runs indefinitely. The returned promise only resolves when the server
    /// is shut down via the `shutdown()` method, or rejects if a fatal connection
    /// error occurs.
    ///
    /// @returns {Promise<void>} A promise that completes when the server stops.
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

    /// Stops the server and closes the WebSocket connection.
    ///
    /// @returns {Promise<void>} A promise that resolves when the shutdown is complete.
    pub async fn shutdown(&self) -> Result<(), JsValue> {
        if let Some(tx) = self.shutdown_tx.lock().unwrap().take() {
            let _ = tx.send(());
        }
        Ok(())
    }
}
