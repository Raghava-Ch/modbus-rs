//! Async Serial (RTU/ASCII) server bindings.

use futures_channel::oneshot;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, Transport, UnitIdOrSlaveAddr,
};
use mbus_serial::WasmSerialTransport;
use wasm_bindgen::prelude::*;

use super::binding_types::WasmSerialServerOptions;
use super::handlers::JsServerHandlers;
use super::task::WasmServerTask;
use crate::wasm::client::helpers::{get_string, get_u8, get_u32};

use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;

#[wasm_bindgen]
/// Browser-facing Modbus serial server (RTU or ASCII) running over Web Serial.
pub struct WasmSerialServer {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>,
    task_fut: Mutex<Option<Pin<Box<dyn Future<Output = Result<(), JsValue>> + Send>>>>,
}

#[wasm_bindgen]
impl WasmSerialServer {
    /// Binds a Web Serial RTU server.
    #[wasm_bindgen(js_name = bindRtu)]
    pub async fn bind_rtu(
        options: WasmSerialServerOptions,
        handlers: JsValue,
    ) -> Result<WasmSerialServer, JsValue> {
        Self::bind_internal::<false>(options, handlers).await
    }

    /// Binds a Web Serial ASCII server.
    #[wasm_bindgen(js_name = bindAscii)]
    pub async fn bind_ascii(
        options: WasmSerialServerOptions,
        handlers: JsValue,
    ) -> Result<WasmSerialServer, JsValue> {
        Self::bind_internal::<true>(options, handlers).await
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

impl WasmSerialServer {
    async fn bind_internal<const ASCII: bool>(
        options: WasmSerialServerOptions,
        handlers: JsValue,
    ) -> Result<WasmSerialServer, JsValue> {
        let options_val = JsValue::from(options);

        let port_val = js_sys::Reflect::get(&options_val, &JsValue::from_str("serialPort"))?;
        if port_val.is_null() || port_val.is_undefined() {
            return Err(JsValue::from_str("Missing or empty 'serialPort'"));
        }

        let unit_id = get_u8(&options_val, "unitId", 1);
        let unit =
            UnitIdOrSlaveAddr::new(unit_id).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let mode = if ASCII {
            SerialMode::Ascii
        } else {
            SerialMode::Rtu
        };

        let baud_rate = get_u32(&options_val, "baudRate", 9600);
        let data_bits = get_u8(&options_val, "dataBits", 8);
        let stop_bits = get_u8(&options_val, "stopBits", 1);
        let parity_str = get_string(&options_val, "parity", "none");
        let response_timeout_ms = get_u32(&options_val, "responseTimeoutMs", 1000);

        let baud = match baud_rate {
            19200 => BaudRate::Baud19200,
            r => BaudRate::Custom(r),
        };

        let db = match data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            _ => DataBits::Eight,
        };

        let pr = match parity_str.as_str() {
            "even" => Parity::Even,
            "odd" => Parity::Odd,
            _ => Parity::None,
        };

        let config = ModbusConfig::Serial(ModbusSerialConfig {
            port_path: heapless::String::try_from("wasm")
                .map_err(|_| JsValue::from_str("port path overflow"))?,
            baud_rate: baud,
            data_bits: db,
            stop_bits,
            parity: pr,
            mode,
            response_timeout_ms,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        // 1. Create and connect transport
        let mut transport = WasmSerialTransport::<ASCII>::new();
        transport.attach_port(port_val);
        Transport::connect(&mut transport, &config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        // 2. Setup channels
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

        // 3. Create server task
        let handlers = JsServerHandlers::new(handlers);
        let task = WasmServerTask::new(transport, handlers, unit, shutdown_rx);
        let task_fut = Box::pin(task.run());

        Ok(WasmSerialServer {
            shutdown_tx: Mutex::new(Some(shutdown_tx)),
            task_fut: Mutex::new(Some(task_fut)),
        })
    }
}
