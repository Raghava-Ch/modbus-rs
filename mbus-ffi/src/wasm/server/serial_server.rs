//! Async Serial (RTU/ASCII) server bindings.

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface WasmSerialServerOptions {
  serialPort: any; // The SerialPort object from navigator.serial
  unitId: number;
  baudRate?: number;
  dataBits?: 7 | 8;
  stopBits?: 1 | 2;
  parity?: "none" | "even" | "odd";
}

export interface ServerHandlers {
  onReadCoils?: (req: { address: number, quantity: number }) => boolean[] | Promise<boolean[]>;
  onReadHoldingRegisters?: (req: { address: number, quantity: number }) => number[] | Promise<number[]>;
  // Add other handlers as needed...
}"#;

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


use std::sync::Mutex;

#[wasm_bindgen]
/// A browser-facing Modbus server that communicates over a serial port using the Web Serial API.
///
/// This class allows you to create a simulated Modbus device (RTU or ASCII) that can be accessed
/// by other applications through a physical or virtual serial port connected to the browser.
/// An instance is created via the static `bindRtu` or `bindAscii` methods.
/// Browser-facing Modbus serial server (RTU or ASCII) running over Web Serial.
pub struct WasmSerialServer {
    shutdown_tx: Mutex<Option<oneshot::Sender<()>>>, // Sender to signal the server task to shut down.
    task_fut: Mutex<Option<std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), JsValue>> + Send>>>>, // The future representing the running server task.
}

#[wasm_bindgen]
impl WasmSerialServer {
    /// Creates and binds a new Modbus RTU server to a browser `SerialPort`.
    ///
    /// @param {WasmSerialServerOptions} options - The server configuration.
    /// @param {object} options.serialPort - The `SerialPort` object obtained from `navigator.serial.requestPort()`.
    /// @param {number} options.unitId - The Modbus unit ID (1-247) the server will respond to.
    /// @param {number} [options.baudRate=9600] - The serial baud rate.
    /// @param {number} [options.dataBits=8] - The number of data bits (7 or 8).
    /// @param {number} [options.stopBits=1] - The number of stop bits (1 or 2).
    /// @param {string} [options.parity="none"] - The parity setting ('none', 'even', or 'odd').
    /// @param {object} handlers - An object containing callback functions to handle incoming Modbus requests (e.g., `onReadHoldingRegisters`).
    /// @returns {Promise<WasmSerialServer>} A promise that resolves to a new `WasmSerialServer` instance.
    ///
    /// @example
    /// const port = await navigator.serial.requestPort();
    /// const server = await WasmSerialServer.bindRtu({ serialPort: port, unitId: 1 }, { onReadHoldingRegisters: () => [1,2,3] });
    #[wasm_bindgen(js_name = bindRtu)]
    pub async fn bind_rtu(
        options: WasmSerialServerOptions,
        handlers: JsValue,
    ) -> Result<WasmSerialServer, JsValue> {
        Self::bind_internal::<false>(options, handlers).await
    }

    /// Creates and binds a new Modbus ASCII server to a browser `SerialPort`.
    ///
    /// @param {WasmSerialServerOptions} options - The server configuration.
    /// @param {object} options.serialPort - The `SerialPort` object obtained from `navigator.serial.requestPort()`.
    /// @param {number} options.unitId - The Modbus unit ID (1-247) the server will respond to.
    /// @param {number} [options.baudRate=9600] - The serial baud rate.
    /// @param {number} [options.dataBits=7] - The number of data bits (7 or 8, defaults to 7 for ASCII).
    /// @param {number} [options.stopBits=1] - The number of stop bits (1 or 2).
    /// @param {string} [options.parity="none"] - The parity setting ('none', 'even', or 'odd').
    /// @param {object} handlers - An object containing callback functions to handle incoming Modbus requests.
    /// @returns {Promise<WasmSerialServer>} A promise that resolves to a new `WasmSerialServer` instance.
    ///
    /// @example
    /// const port = await navigator.serial.requestPort();
    /// const server = await WasmSerialServer.bindAscii({ serialPort: port, unitId: 1, baudRate: 19200 }, { onReadCoils: () => [true, false] });
    #[wasm_bindgen(js_name = bindAscii)]
    pub async fn bind_ascii(
        options: WasmSerialServerOptions,
        handlers: JsValue,
    ) -> Result<WasmSerialServer, JsValue> {
        Self::bind_internal::<true>(options, handlers).await
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

    /// Stops the server and releases the serial port.
    ///
    /// @returns {Promise<void>} A promise that resolves when the shutdown is complete.
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
