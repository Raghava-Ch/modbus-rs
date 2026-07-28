//! Browser Web Serial support for WASM.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use futures_util::FutureExt;
use js_sys::{Function, Promise, Reflect};
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, TransportType, UnitIdOrSlaveAddr,
};
use mbus_serial::{
    WasmAsciiTransport as WasmAsciiTransportInner, WasmRtuTransport as WasmRtuTransportInner,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::client_tcp::CreateClientOptions;
use super::command::WasmCommand;
use super::helpers::*;
use super::response::WasmResponse;
use super::task::{WasmAsyncTransportTrait, WasmClientTask};
use crate::wasm::wasm_types::{
    DiagnosticsOptions, MaskWriteRegisterOptions, ReadBitsOptions, ReadDeviceIdentificationOptions,
    ReadFifoQueueOptions, ReadFileRecordOptions, ReadRegistersOptions,
    ReadWriteMultipleRegistersOptions, WriteFileRecordOptions, WriteMultipleCoilsOptions,
    WriteMultipleRegistersOptions, WriteSingleCoilOptions, WriteSingleRegisterOptions,
};
use wasm_bindgen::closure::Closure;

#[cfg(feature = "file-record")]
use mbus_client::services::file_record::SubRequestParams;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface WasmSerialTransportOptions {
  /** @default 9600 */
  baudRate?: number
  /** @default 8 */
  dataBits?: 7 | 8
  /** @default 1 */
  stopBits?: 1 | 2
  /** @default "none" */
  parity?: "none" | "even" | "odd"
  /**
   * The maximum time in milliseconds to wait for a response.
   * @default 1000
   */
  requestTimeoutMs?: number
}"#;

#[wasm_bindgen]
extern "C" {
    /// Options for creating a `WasmSerialTransport`.
    ///
    /// ```typescript
    /// interface WasmSerialTransportOptions {
    ///   /**
    ///    * The serial mode to use.
    ///    * @default "rtu"
    ///    */
    ///   mode?: "rtu" | "ascii";
    ///   /** @default 9600 */
    ///   baudRate?: number;
    ///   /** @default 8 */
    ///   dataBits?: 7 | 8;
    ///   /** @default 1 */
    ///   stopBits?: 1 | 2;
    ///   /** @default "none" */
    ///   parity?: "none" | "even" | "odd";
    ///   /**
    ///    * The maximum time in milliseconds to wait for a response.
    ///    * @default 1000
    ///    */
    ///   responseTimeoutMs?: number;
    /// }
    /// ```
    #[wasm_bindgen(typescript_type = "WasmSerialTransportOptions")]
    pub type WasmSerialTransportOptions;
}

// ── WasmRuntimeSerialTransport ───────────────────────────────────────────────

enum WasmRuntimeSerialTransport {
    Rtu(WasmRtuTransportInner),
    Ascii(WasmAsciiTransportInner),
}

impl WasmRuntimeSerialTransport {
    fn new(mode: SerialMode) -> Self {
        match mode {
            SerialMode::Rtu => Self::Rtu(WasmRtuTransportInner::new()),
            SerialMode::Ascii => Self::Ascii(WasmAsciiTransportInner::new()),
        }
    }

    fn attach_port(&mut self, port: JsValue) {
        match self {
            Self::Rtu(transport) => transport.attach_port(port),
            Self::Ascii(transport) => transport.attach_port(port),
        }
    }
    fn connect(
        &mut self,
        config: &ModbusConfig,
    ) -> Result<(), mbus_core::transport::TransportError> {
        use mbus_core::transport::Transport;
        match self {
            Self::Rtu(t) => t.connect(config),
            Self::Ascii(t) => t.connect(config),
        }
    }

    fn disconnect(&mut self) -> Result<(), mbus_core::transport::TransportError> {
        use mbus_core::transport::Transport;
        match self {
            Self::Rtu(t) => t.disconnect(),
            Self::Ascii(t) => t.disconnect(),
        }
    }
}

impl WasmAsyncTransportTrait for WasmRuntimeSerialTransport {
    async fn recv_frame(
        &mut self,
    ) -> Result<
        heapless::Vec<u8, { mbus_core::data_unit::common::MAX_ADU_FRAME_LEN }>,
        mbus_core::errors::MbusError,
    > {
        match self {
            Self::Rtu(t) => t.recv_frame().await,
            Self::Ascii(t) => t.recv_frame().await,
        }
    }

    fn send_frame(&mut self, adu: &[u8]) -> Result<(), mbus_core::errors::MbusError> {
        match self {
            Self::Rtu(t) => t.send_frame(adu),
            Self::Ascii(t) => t.send_frame(adu),
        }
    }
}

// ── WasmSerialPortHandle ─────────────────────────────────────────────────────

#[wasm_bindgen]
/// Opaque handle around a browser `SerialPort` object granted by Web Serial.
pub struct WasmSerialPortHandle {
    port: JsValue,
}

#[wasm_bindgen]
impl WasmSerialPortHandle {
    /// Returns true if the wrapped JS value still looks like a valid SerialPort object.
    #[wasm_bindgen(js_name = "isValid")]
    pub fn is_valid(&self) -> bool {
        !self.port.is_null() && !self.port.is_undefined()
    }
}

impl WasmSerialPortHandle {
    /// Clone the internal port handle value.
    pub fn clone_port(&self) -> JsValue {
        self.port.clone()
    }
}

impl WasmSerialPortHandle {
    /// Construct a handle wrapping any JS value.
    #[doc(hidden)]
    pub fn new_for_testing(port: JsValue) -> Self {
        WasmSerialPortHandle { port }
    }
}

/// Requests a browser serial port from `navigator.serial.requestPort()`.
///
/// This function must be called from within a user gesture handler, such as a button click.
/// It prompts the user to select a serial port, which is then returned as an opaque handle.
///
/// @returns {Promise<WasmSerialPortHandle>} A promise that resolves with the port handle.
/// @throws {Error} If the Web Serial API is not available or the user cancels the request.
///
/// @example
/// ```javascript
/// document.getElementById('connect-button').addEventListener('click', async () => {
///   try {
///     const portHandle = await request_serial_port();
///     // Now use this handle to create a WasmSerialTransport
///     const transport = new WasmSerialTransport(portHandle, { baudRate: 19200 });
///   } catch (e) {
///     console.error("Failed to get serial port:", e);
///   }
/// });
/// ```
#[wasm_bindgen(js_name = "requestSerialPort")]
pub async fn request_serial_port() -> Result<WasmSerialPortHandle, JsValue> {
    let global = js_sys::global();
    let navigator = Reflect::get(&global, &JsValue::from_str("navigator"))?;
    let serial = Reflect::get(&navigator, &JsValue::from_str("serial"))?;

    // Check for HTTPS/localhost context, which is required by Web Serial.
    if serial.is_null() || serial.is_undefined() {
        return Err(JsValue::from_str(
            "Web Serial API unavailable. Use a Chromium-based browser over HTTPS/localhost.",
        ));
    }

    let request_port = Reflect::get(&serial, &JsValue::from_str("requestPort"))?
        .dyn_into::<Function>()
        .map_err(|_| JsValue::from_str("navigator.serial.requestPort is not callable"))?;

    let promise = request_port
        .call0(&serial)?
        .dyn_into::<Promise>()
        .map_err(|_| JsValue::from_str("requestPort did not return a Promise"))?;

    let port = JsFuture::from(promise).await?;
    Ok(WasmSerialPortHandle { port })
}

// ── WasmRtuTransport ─────────────────────────────────────────────────────────

#[wasm_bindgen(skip_typescript)]
/// Connection manager for browser Modbus RTU clients using the Web Serial API.
pub struct WasmRtuTransport {
    _port_handle: WasmSerialPortHandle,
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    pending_count: Rc<Cell<usize>>,
    active_transport: Rc<RefCell<Option<WasmRuntimeSerialTransport>>>,
    default_timeout_ms: u32,
    current_timeout_ms: Rc<Cell<u32>>,
    config: ModbusConfig,
}

#[wasm_bindgen]
impl WasmRtuTransport {
    /// Opens the serial port in RTU mode.
    #[wasm_bindgen(js_name = "open")]
    pub fn open(
        port_handle: WasmSerialPortHandle,
        options: Option<WasmSerialTransportOptions>,
    ) -> Promise {
        let (promise, resolve, reject) = make_promise();
        match Self::open_rust(port_handle, options) {
            Ok(t) => {
                let _ = resolve.call1(&JsValue::NULL, &t.into());
            }
            Err(e) => {
                let _ = reject.call1(&JsValue::NULL, &e);
            }
        }
        promise
    }

    fn open_rust(
        port_handle: WasmSerialPortHandle,
        options: Option<WasmSerialTransportOptions>,
    ) -> Result<WasmRtuTransport, JsValue> {
        let options_val = options.map(JsValue::from).unwrap_or(JsValue::UNDEFINED);
        let baud_rate = get_u32(&options_val, "baudRate", 9600);
        let data_bits = get_u8(&options_val, "dataBits", 8);
        let stop_bits = get_u8(&options_val, "stopBits", 1);
        let parity_str = get_string(&options_val, "parity", "none");
        let request_timeout_ms = get_u32(&options_val, "requestTimeoutMs", 1000);
        let retry_attempts = get_u8(&options_val, "retryAttempts", 0);

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
            mode: SerialMode::Rtu,
            response_timeout_ms: request_timeout_ms,
            retry_attempts,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let mut transport = WasmRuntimeSerialTransport::new(SerialMode::Rtu);
        transport.attach_port(port_handle.clone_port());

        transport
            .connect(&config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let (cmd_tx, cmd_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
        let pending_count = Rc::new(Cell::new(0));
        let active_transport = Rc::new(RefCell::new(Some(transport)));

        let active_transport_clone = active_transport.clone();
        spawn_local(async move {
            let transport_opt = active_transport_clone.borrow_mut().take();
            if let Some(t) = transport_opt {
                let task =
                    WasmClientTask::new(t, cmd_rx, TransportType::CustomSerial(SerialMode::Rtu));
                task.run().await;
            }
        });

        let default_timeout_ms = request_timeout_ms;
        let current_timeout_ms = Rc::new(Cell::new(default_timeout_ms));

        Ok(WasmRtuTransport {
            _port_handle: port_handle,
            cmd_tx: Rc::new(RefCell::new(cmd_tx)),
            pending_count,
            active_transport,
            default_timeout_ms,
            current_timeout_ms,
            config,
        })
    }

    /// Returns `true` if there are any in-flight Modbus requests pending a response.
    #[wasm_bindgen(getter, js_name = "pendingRequests")]
    pub fn pending_requests(&self) -> bool {
        self.pending_count.get() > 0
    }

    /// Closes the serial port connection and terminates the background task.
    #[wasm_bindgen]
    pub fn close(&mut self) -> Promise {
        *self.cmd_tx.borrow_mut() = futures_channel::mpsc::unbounded::<WasmCommand>().0;
        self.pending_count.set(0);
        if let Some(t) = self.active_transport.borrow_mut().as_mut() {
            let _ = t.disconnect();
        }
        Promise::resolve(&JsValue::UNDEFINED)
    }

    /// Sets a temporary request timeout override (in milliseconds) for all clients of this transport.
    #[wasm_bindgen(js_name = "setRequestTimeout")]
    pub fn set_request_timeout(&self, ms: u32) {
        self.current_timeout_ms.set(ms);
    }

    /// Clears any request timeout override and restores the default timeout.
    #[wasm_bindgen(js_name = "clearRequestTimeout")]
    pub fn clear_request_timeout(&self) {
        self.current_timeout_ms.set(self.default_timeout_ms);
    }

    /// Drop all pending in-flight requests and attempt to reconnect.
    #[wasm_bindgen(js_name = "reconnect")]
    pub fn reconnect(&mut self) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let port_val = self._port_handle.clone_port();
        let cmd_tx_cell = self.cmd_tx.clone();
        let pending_count_cell = self.pending_count.clone();
        let active_transport_cell = self.active_transport.clone();
        let config = self.config.clone();

        spawn_local(async move {
            let mut transport = WasmRuntimeSerialTransport::new(SerialMode::Rtu);
            transport.attach_port(port_val);

            match transport.connect(&config) {
                Ok(_) => {
                    let (new_tx, new_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
                    *cmd_tx_cell.borrow_mut() = new_tx;
                    *active_transport_cell.borrow_mut() = Some(transport);

                    let active_transport_clone = active_transport_cell.clone();
                    spawn_local(async move {
                        let transport_opt = active_transport_clone.borrow_mut().take();
                        if let Some(t) = transport_opt {
                            let task = WasmClientTask::new(
                                t,
                                new_rx,
                                TransportType::CustomSerial(SerialMode::Rtu),
                            );
                            task.run().await;
                        }
                    });

                    pending_count_cell.set(0);
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(err) => {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&format!("{:?}", err)));
                }
            }
        });

        promise
    }

    /// Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
    #[wasm_bindgen(js_name = "createClient")]
    pub fn create_client(
        &self,
        options: CreateClientOptions,
    ) -> Result<WasmSerialModbusClient, JsValue> {
        let options_val = JsValue::from(options);
        if options_val.is_null() || options_val.is_undefined() {
            return Err(JsValue::from_str(
                "Missing options object. unitId is required.",
            ));
        }
        let unit_id_val = Reflect::get(&options_val, &JsValue::from_str("unitId"))
            .map_err(|_| JsValue::from_str("Missing property 'unitId'"))?;
        if unit_id_val.is_null() || unit_id_val.is_undefined() {
            return Err(JsValue::from_str("Property 'unitId' is required"));
        }
        let unit_id = unit_id_val
            .as_f64()
            .ok_or_else(|| JsValue::from_str("unitId must be a number"))?
            as u8;

        UnitIdOrSlaveAddr::new(unit_id).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(WasmSerialModbusClient {
            cmd_tx: self.cmd_tx.clone(),
            unit_id,
            pending_count: self.pending_count.clone(),
            response_timeout_ms: self.current_timeout_ms.clone(),
        })
    }
}

// ── WasmAsciiTransport ───────────────────────────────────────────────────────

#[wasm_bindgen(skip_typescript)]
/// Connection manager for browser Modbus ASCII clients using the Web Serial API.
pub struct WasmAsciiTransport {
    _port_handle: WasmSerialPortHandle,
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    pending_count: Rc<Cell<usize>>,
    active_transport: Rc<RefCell<Option<WasmRuntimeSerialTransport>>>,
    default_timeout_ms: u32,
    current_timeout_ms: Rc<Cell<u32>>,
    config: ModbusConfig,
}

#[wasm_bindgen]
impl WasmAsciiTransport {
    /// Opens the serial port in ASCII mode.
    #[wasm_bindgen(js_name = "open")]
    pub fn open(
        port_handle: WasmSerialPortHandle,
        options: Option<WasmSerialTransportOptions>,
    ) -> Promise {
        let (promise, resolve, reject) = make_promise();
        match Self::open_rust(port_handle, options) {
            Ok(t) => {
                let _ = resolve.call1(&JsValue::NULL, &t.into());
            }
            Err(e) => {
                let _ = reject.call1(&JsValue::NULL, &e);
            }
        }
        promise
    }

    fn open_rust(
        port_handle: WasmSerialPortHandle,
        options: Option<WasmSerialTransportOptions>,
    ) -> Result<WasmAsciiTransport, JsValue> {
        let options_val = options.map(JsValue::from).unwrap_or(JsValue::UNDEFINED);
        let baud_rate = get_u32(&options_val, "baudRate", 9600);
        let data_bits = get_u8(&options_val, "dataBits", 8);
        let stop_bits = get_u8(&options_val, "stopBits", 1);
        let parity_str = get_string(&options_val, "parity", "none");
        let request_timeout_ms = get_u32(&options_val, "requestTimeoutMs", 1000);
        let retry_attempts = get_u8(&options_val, "retryAttempts", 0);

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
            mode: SerialMode::Ascii,
            response_timeout_ms: request_timeout_ms,
            retry_attempts,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let mut transport = WasmRuntimeSerialTransport::new(SerialMode::Ascii);
        transport.attach_port(port_handle.clone_port());

        transport
            .connect(&config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let (cmd_tx, cmd_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
        let pending_count = Rc::new(Cell::new(0));
        let active_transport = Rc::new(RefCell::new(Some(transport)));

        let active_transport_clone = active_transport.clone();
        spawn_local(async move {
            let transport_opt = active_transport_clone.borrow_mut().take();
            if let Some(t) = transport_opt {
                let task =
                    WasmClientTask::new(t, cmd_rx, TransportType::CustomSerial(SerialMode::Ascii));
                task.run().await;
            }
        });

        let default_timeout_ms = request_timeout_ms;
        let current_timeout_ms = Rc::new(Cell::new(default_timeout_ms));

        Ok(WasmAsciiTransport {
            _port_handle: port_handle,
            cmd_tx: Rc::new(RefCell::new(cmd_tx)),
            pending_count,
            active_transport,
            default_timeout_ms,
            current_timeout_ms,
            config,
        })
    }

    /// Returns `true` if there are any in-flight Modbus requests pending a response.
    #[wasm_bindgen(getter, js_name = "pendingRequests")]
    pub fn pending_requests(&self) -> bool {
        self.pending_count.get() > 0
    }

    /// Closes the serial port connection and terminates the background task.
    #[wasm_bindgen]
    pub fn close(&mut self) -> Promise {
        *self.cmd_tx.borrow_mut() = futures_channel::mpsc::unbounded::<WasmCommand>().0;
        self.pending_count.set(0);
        if let Some(t) = self.active_transport.borrow_mut().as_mut() {
            let _ = t.disconnect();
        }
        Promise::resolve(&JsValue::UNDEFINED)
    }

    /// Sets a temporary request timeout override (in milliseconds) for all clients of this transport.
    #[wasm_bindgen(js_name = "setRequestTimeout")]
    pub fn set_request_timeout(&self, ms: u32) {
        self.current_timeout_ms.set(ms);
    }

    /// Clears any request timeout override and restores the default timeout.
    #[wasm_bindgen(js_name = "clearRequestTimeout")]
    pub fn clear_request_timeout(&self) {
        self.current_timeout_ms.set(self.default_timeout_ms);
    }

    /// Drop all pending in-flight requests and attempt to reconnect.
    #[wasm_bindgen(js_name = "reconnect")]
    pub fn reconnect(&mut self) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let port_val = self._port_handle.clone_port();
        let cmd_tx_cell = self.cmd_tx.clone();
        let pending_count_cell = self.pending_count.clone();
        let active_transport_cell = self.active_transport.clone();
        let config = self.config.clone();

        spawn_local(async move {
            let mut transport = WasmRuntimeSerialTransport::new(SerialMode::Ascii);
            transport.attach_port(port_val);

            match transport.connect(&config) {
                Ok(_) => {
                    let (new_tx, new_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
                    *cmd_tx_cell.borrow_mut() = new_tx;
                    *active_transport_cell.borrow_mut() = Some(transport);

                    let active_transport_clone = active_transport_cell.clone();
                    spawn_local(async move {
                        let transport_opt = active_transport_clone.borrow_mut().take();
                        if let Some(t) = transport_opt {
                            let task = WasmClientTask::new(
                                t,
                                new_rx,
                                TransportType::CustomSerial(SerialMode::Ascii),
                            );
                            task.run().await;
                        }
                    });

                    pending_count_cell.set(0);
                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(err) => {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&format!("{:?}", err)));
                }
            }
        });

        promise
    }

    /// Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
    #[wasm_bindgen(js_name = "createClient")]
    pub fn create_client(
        &self,
        options: CreateClientOptions,
    ) -> Result<WasmSerialModbusClient, JsValue> {
        let options_val = JsValue::from(options);
        if options_val.is_null() || options_val.is_undefined() {
            return Err(JsValue::from_str(
                "Missing options object. unitId is required.",
            ));
        }
        let unit_id_val = Reflect::get(&options_val, &JsValue::from_str("unitId"))
            .map_err(|_| JsValue::from_str("Missing property 'unitId'"))?;
        if unit_id_val.is_null() || unit_id_val.is_undefined() {
            return Err(JsValue::from_str("Property 'unitId' is required"));
        }
        let unit_id = unit_id_val
            .as_f64()
            .ok_or_else(|| JsValue::from_str("unitId must be a number"))?
            as u8;

        UnitIdOrSlaveAddr::new(unit_id).map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        Ok(WasmSerialModbusClient {
            cmd_tx: self.cmd_tx.clone(),
            unit_id,
            pending_count: self.pending_count.clone(),
            response_timeout_ms: self.current_timeout_ms.clone(),
        })
    }
}

// ── WasmSerialModbusClient ───────────────────────────────────────────────────

#[wasm_bindgen(skip_typescript)]
/// A browser-facing Modbus serial client bound to a specific unit ID (slave address).
///
/// This class provides methods for all standard Modbus function codes. All operations
/// are asynchronous and return a `Promise`. It is created via `WasmSerialTransport.createClient()`.
pub struct WasmSerialModbusClient {
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    unit_id: u8,
    pending_count: Rc<Cell<usize>>,
    response_timeout_ms: Rc<Cell<u32>>,
}

enum SelectedResult {
    Response(Result<Result<WasmResponse, String>, futures_channel::oneshot::Canceled>),
    Timeout,
    Abort,
}

#[wasm_bindgen]
impl WasmSerialModbusClient {
    /// Returns `true` if there are any in-flight Modbus requests pending a response for this client.
    #[wasm_bindgen(getter, js_name = "pendingRequests")]
    pub fn pending_requests(&self) -> bool {
        self.pending_count.get() > 0
    }

    /// Returns `true` if the underlying transport is considered connected.
    #[wasm_bindgen(js_name = "isConnected")]
    pub fn is_connected(&self) -> bool {
        !self.cmd_tx.borrow().is_closed()
    }

    // Helper to dispatch a command and return a Promise
    fn dispatch(
        &self,
        cmd: WasmCommand,
        rx: futures_channel::oneshot::Receiver<Result<WasmResponse, String>>,
        signal: &JsValue,
    ) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let pending_count = self.pending_count.clone();
        pending_count.set(pending_count.get() + 1);

        if !signal.is_null() && !signal.is_undefined() {
            let aborted = Reflect::get(signal, &JsValue::from_str("aborted"))
                .ok()
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if aborted {
                pending_count.set(pending_count.get() - 1);
                let abort_err = js_sys::Error::new("The operation was aborted.");
                let _ = abort_err.set_name("AbortError");
                let _ = reject.call1(&JsValue::NULL, &abort_err);
                return promise;
            }
        }

        if self.cmd_tx.borrow().unbounded_send(cmd).is_err() {
            pending_count.set(pending_count.get() - 1);
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("ConnectionClosed"));
            return promise;
        }

        let timeout_ms = self.response_timeout_ms.get();
        let signal_clone = signal.clone();

        spawn_local(async move {
            let timeout_fut = gloo_timers::future::TimeoutFuture::new(timeout_ms);

            let mut rx_abort = None;
            let mut _on_abort_closure = None;
            if !signal_clone.is_null() && !signal_clone.is_undefined() {
                let (tx_abort, rx_abort_inner) = futures_channel::oneshot::channel::<()>();
                rx_abort = Some(rx_abort_inner);
                let tx_abort_cell = RefCell::new(Some(tx_abort));
                let on_abort = Closure::wrap(Box::new(move || {
                    if let Some(tx) = tx_abort_cell.borrow_mut().take() {
                        let _ = tx.send(());
                    }
                }) as Box<dyn FnMut()>);

                if let Ok(add_event_listener) =
                    Reflect::get(&signal_clone, &JsValue::from_str("addEventListener"))
                {
                    if let Ok(add_event_listener_fn) =
                        add_event_listener.dyn_into::<js_sys::Function>()
                    {
                        let _ = add_event_listener_fn.call2(
                            &signal_clone,
                            &JsValue::from_str("abort"),
                            on_abort.as_ref(),
                        );
                    }
                }
                _on_abort_closure = Some(on_abort);
            }

            let mut rx_fuse = rx.fuse();
            let mut timeout_fuse = timeout_fut.fuse();

            let res = if let Some(ref mut rx_abort_fut) = rx_abort {
                let mut rx_abort_fuse = rx_abort_fut.fuse();
                futures_util::select! {
                    res = rx_fuse => {
                        SelectedResult::Response(res)
                    }
                    _ = timeout_fuse => {
                        SelectedResult::Timeout
                    }
                    _ = rx_abort_fuse => {
                        SelectedResult::Abort
                    }
                }
            } else {
                futures_util::select! {
                    res = rx_fuse => {
                        SelectedResult::Response(res)
                    }
                    _ = timeout_fuse => {
                        SelectedResult::Timeout
                    }
                }
            };

            if let Some(closure) = _on_abort_closure {
                if let Ok(remove_event_listener) =
                    Reflect::get(&signal_clone, &JsValue::from_str("removeEventListener"))
                {
                    if let Ok(remove_event_listener_fn) =
                        remove_event_listener.dyn_into::<js_sys::Function>()
                    {
                        let _ = remove_event_listener_fn.call2(
                            &signal_clone,
                            &JsValue::from_str("abort"),
                            closure.as_ref(),
                        );
                    }
                }
            }

            pending_count.set(pending_count.get() - 1);
            match res {
                SelectedResult::Response(Ok(Ok(resp))) => {
                    let _ = resolve.call1(&JsValue::NULL, &resp.to_js_value());
                }
                SelectedResult::Response(Ok(Err(err))) => {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&err));
                }
                SelectedResult::Response(Err(_)) => {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("ConnectionLost"));
                }
                SelectedResult::Timeout => {
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("Timeout"));
                }
                SelectedResult::Abort => {
                    let abort_err = js_sys::Error::new("The operation was aborted.");
                    let _ = abort_err.set_name("AbortError");
                    let _ = reject.call1(&JsValue::NULL, &abort_err);
                }
            }
        });

        promise
    }

    // ── Coil operations ──────────────────────────────────────────────────────

    /// Reads a sequence of coils (Function Code 01).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the coils to read (0-based).
    /// @param {number} options.quantity - The number of coils to read (1-125).
    /// @returns {Promise<boolean[]>} A promise that resolves to an array of booleans representing the coil states.
    ///
    /// @example
    /// ```javascript
    /// const coils = await client.readCoils({ address: 0, quantity: 8 });
    /// console.log(coils); // e.g., [true, false, true, ...]
    /// ```
    #[wasm_bindgen(js_name = "readCoils", skip_typescript)]
    pub fn read_coils(&mut self, options: ReadBitsOptions) -> Promise {
        let address = options.address;
        let quantity = options.quantity;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadCoils {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Writes a single coil state (Function Code 05).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The address of the coil to write (0-based).
    /// @param {boolean} options.value - The state to write (`true` for ON, `false` for OFF).
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeSingleCoil({ address: 10, value: true });
    /// ```
    #[wasm_bindgen(js_name = "writeSingleCoil", skip_typescript)]
    pub fn write_single_coil(&mut self, options: WriteSingleCoilOptions) -> Promise {
        let address = options.address;
        let value = if options.value {
            mbus_core::models::coil::CoilState::On
        } else {
            mbus_core::models::coil::CoilState::Off
        };
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteSingleCoil {
            unit_id,
            address,
            value,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Writes a sequence of coil states (Function Code 15).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the coils to write (0-based).
    /// @param {boolean[]} options.values - An array of boolean states to write.
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeMultipleCoils({ address: 20, values: [true, false, true, true] });
    /// ```
    #[wasm_bindgen(js_name = "writeMultipleCoils", skip_typescript)]
    pub fn write_multiple_coils(&mut self, options: WriteMultipleCoilsOptions) -> Promise {
        let address = options.address;
        let values = options
            .values
            .into_iter()
            .map(|b| {
                if b {
                    mbus_core::models::coil::CoilState::On
                } else {
                    mbus_core::models::coil::CoilState::Off
                }
            })
            .collect();
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteMultipleCoils {
            unit_id,
            address,
            values,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Reads a sequence of discrete inputs (Function Code 02).
    ///
    /// These are read-only boolean inputs.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the inputs to read (0-based).
    /// @param {number} options.quantity - The number of inputs to read (1-125).
    /// @returns {Promise<boolean[]>} A promise that resolves to an array of booleans.
    ///
    /// @example
    /// ```javascript
    /// const inputs = await client.readDiscreteInputs({ address: 0, quantity: 4 });
    /// ```
    #[wasm_bindgen(js_name = "readDiscreteInputs", skip_typescript)]
    pub fn read_discrete_inputs(&mut self, options: ReadBitsOptions) -> Promise {
        let address = options.address;
        let quantity = options.quantity;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadDiscreteInputs {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    // ── Register operations ───────────────────────────────────────────────────

    /// Reads a sequence of holding registers (Function Code 03).
    ///
    /// These are 16-bit read/write registers.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the registers to read (0-based).
    /// @param {number} options.quantity - The number of registers to read (1-125).
    /// @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
    ///
    /// @example
    /// ```javascript
    /// const regs = await client.readHoldingRegisters({ address: 100, quantity: 10 });
    /// ```
    #[wasm_bindgen(js_name = "readHoldingRegisters", skip_typescript)]
    pub fn read_holding_registers(&mut self, options: ReadRegistersOptions) -> Promise {
        let address = options.address;
        let quantity = options.quantity;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadHoldingRegisters {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Reads a sequence of input registers (Function Code 04).
    ///
    /// These are 16-bit read-only registers.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the registers to read (0-based).
    /// @param {number} options.quantity - The number of registers to read (1-125).
    /// @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of register values.
    ///
    /// @example
    /// ```javascript
    /// const inputRegs = await client.readInputRegisters({ address: 50, quantity: 2 });
    /// ```
    #[wasm_bindgen(js_name = "readInputRegisters", skip_typescript)]
    pub fn read_input_registers(&mut self, options: ReadRegistersOptions) -> Promise {
        let address = options.address;
        let quantity = options.quantity;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadInputRegisters {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Writes a single holding register (Function Code 06).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The address of the register to write (0-based).
    /// @param {number} options.value - The 16-bit value to write.
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeSingleRegister({ address: 100, value: 42 });
    /// ```
    #[wasm_bindgen(js_name = "writeSingleRegister", skip_typescript)]
    pub fn write_single_register(&mut self, options: WriteSingleRegisterOptions) -> Promise {
        let address = options.address;
        let value = options.value;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteSingleRegister {
            unit_id,
            address,
            value,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Writes a sequence of holding registers (Function Code 16).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the registers to write (0-based).
    /// @param {Uint16Array} options.values - An array of 16-bit values to write.
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeMultipleRegisters({ address: 200, values: });
    /// await client.writeMultipleRegisters({ address: 210, values: Uint16Array.from() });
    /// ```
    #[wasm_bindgen(js_name = "writeMultipleRegisters", skip_typescript)]
    pub fn write_multiple_registers(&mut self, options: WriteMultipleRegistersOptions) -> Promise {
        let address = options.address;
        let values = options.values;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteMultipleRegisters {
            unit_id,
            address,
            values,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Performs an atomic read and write of holding registers in a single transaction (Function Code 23).
    ///
    /// The write operation is performed before the read.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.readAddress - The starting address for the read operation.
    /// @param {number} options.readQuantity - The number of registers to read.
    /// @param {number} options.writeAddress - The starting address for the write operation.
    /// @param {Uint16Array} options.writeValues - The values to write.
    /// @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of the registers read.
    ///
    /// @example
    /// ```javascript
    /// const readData = await client.readWriteMultipleRegisters({
    ///   readAddress: 10, readQuantity: 2, writeAddress: 20, writeValues:
    /// });
    /// ```
    #[wasm_bindgen(js_name = "readWriteMultipleRegisters", skip_typescript)]
    pub fn read_write_multiple_registers(
        &mut self,
        options: ReadWriteMultipleRegistersOptions,
    ) -> Promise {
        let read_address = options.read_address;
        let read_quantity = options.read_quantity;
        let write_address = options.write_address;
        let write_values = options.write_values;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadWriteMultipleRegisters {
            unit_id,
            read_address,
            read_quantity,
            write_address,
            write_values,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Modifies a single holding register using a bitwise AND/OR mask (Function Code 22).
    ///
    /// The operation is `(current_value AND andMask) OR (orMask AND (NOT andMask))`.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The address of the register to modify.
    /// @param {number} options.andMask - The bitwise AND mask.
    /// @param {number} options.orMask - The bitwise OR mask.
    /// @returns {Promise<void>} A promise that resolves when the operation is complete.
    ///
    /// @example
    /// ```javascript
    /// // Set bits 0-7 and clear bits 8-15 of the register at address 300
    /// await client.maskWriteRegister({
    ///   address: 300, andMask: 0x00FF, orMask: 0xFF00
    /// });
    /// ```
    #[wasm_bindgen(js_name = "maskWriteRegister", skip_typescript)]
    pub fn mask_write_register(&mut self, options: MaskWriteRegisterOptions) -> Promise {
        let address = options.address;
        let and_mask = options.and_mask;
        let or_mask = options.or_mask;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::MaskWriteRegister {
            unit_id,
            address,
            and_mask,
            or_mask,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    // ── FIFO queue operations ─────────────────────────────────────────────────

    /// Reads the contents of a FIFO queue of 16-bit registers (Function Code 18).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The address of the FIFO queue.
    /// @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of the queue contents.
    ///
    /// @example
    /// ```javascript
    /// const fifoContents = await client.readFifoQueue({ address: 42 });
    /// ```
    #[wasm_bindgen(js_name = "readFifoQueue", skip_typescript)]
    #[cfg(feature = "fifo")]
    pub fn read_fifo_queue(&mut self, options: ReadFifoQueueOptions) -> Promise {
        let address = options.address;
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadFifoQueue {
            unit_id,
            address,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    // ── File record operations ────────────────────────────────────────────────

    /// Reads one or more file records (Function Code 14).
    ///
    /// @param {object} options - The request parameters.
    /// @param {object[]} options.requests - An array of sub-request objects.
    /// @param {number} options.requests[].fileNumber - The file number.
    /// @param {number} options.requests[].recordNumber - The starting record number within the file.
    /// @param {number} options.requests[].recordLength - The number of registers to read for this record.
    /// @returns {Promise<Uint16Array[]>} A promise that resolves to an array of `Uint16Array`, with each element corresponding to a sub-request.
    ///
    /// @example
    /// ```javascript
    /// const records = await client.readFileRecord({
    ///   requests: [
    ///     { fileNumber: 4, recordNumber: 1, recordLength: 2 },
    ///     { fileNumber: 3, recordNumber: 0, recordLength: 5 }
    ///   ]
    /// });
    /// ```
    #[wasm_bindgen(js_name = "readFileRecord", skip_typescript)]
    #[cfg(feature = "file-record")]
    pub fn read_file_record(&mut self, options: ReadFileRecordOptions) -> Promise {
        let requests = options
            .requests
            .into_iter()
            .map(|r| SubRequestParams {
                file_number: r.file_number,
                record_number: r.record_number,
                record_length: r.record_length,
                record_data: None,
            })
            .collect();

        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadFileRecord {
            unit_id,
            requests,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Writes one or more file records (Function Code 15).
    ///
    /// @param {object} options - The request parameters.
    /// @param {object[]} options.requests - An array of sub-request objects to write.
    /// @param {number} options.requests[].fileNumber - The file number.
    /// @param {number} options.requests[].recordNumber - The starting record number within the file.
    /// @param {Uint16Array} options.requests[].recordData - The register data to write.
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeFileRecord({
    ///   requests: [
    ///     { fileNumber: 4, recordNumber: 1, recordData: [0xDEAD, 0xBEEF] }
    ///   ]
    /// });
    /// ```
    #[wasm_bindgen(js_name = "writeFileRecord", skip_typescript)]
    #[cfg(feature = "file-record")]
    pub fn write_file_record(&mut self, options: WriteFileRecordOptions) -> Promise {
        let mut requests = Vec::new();
        for r in options.requests {
            let record_length = r.record_data.len() as u16;
            let mut hv_data = heapless::Vec::new();
            if hv_data.extend_from_slice(&r.record_data).is_err() {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(
                    &JsValue::NULL,
                    &JsValue::from_str("Too many registers in recordData"),
                );
                return promise;
            }
            requests.push(SubRequestParams {
                file_number: r.file_number,
                record_number: r.record_number,
                record_length,
                record_data: Some(hv_data),
            });
        }

        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteFileRecord {
            unit_id,
            requests,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    // ── Diagnostics operations ────────────────────────────────────────────────

    /// Reads the device's exception status (Function Code 07).
    ///
    /// The result is an 8-bit value where each bit corresponds to a specific exception flag.
    ///
    /// @returns {Promise<number>} A promise that resolves to the 8-bit exception status.
    ///
    /// @example
    /// const status = await client.readExceptionStatus();
    #[wasm_bindgen(js_name = "readExceptionStatus", skip_typescript)]
    #[cfg(feature = "diagnostics")]
    pub fn read_exception_status(&mut self) -> Promise {
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadExceptionStatus { unit_id, resp: tx };
        self.dispatch(cmd, rx, &JsValue::UNDEFINED)
    }

    /// Performs a diagnostic function on the device (Function Code 08).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.subFunction - The diagnostic sub-function code to execute.
    /// @param {Uint16Array} [options.data] - Optional data to send with the request.
    /// @returns {Promise<object>} A promise that resolves to an object containing the `subFunction` and `data` (`Uint16Array`) from the response.
    ///
    /// @example
    /// ```javascript
    /// // Example: Return query data
    /// const response = await client.diagnostics({
    ///   subFunction: 0,
    ///   data: [0x12, 0x34]
    /// });
    /// ```
    #[wasm_bindgen(js_name = "diagnostics", skip_typescript)]
    #[cfg(feature = "diagnostics")]
    pub fn diagnostics(&mut self, options: DiagnosticsOptions) -> Promise {
        let sub_function = options.sub_function;
        let data = options.data.unwrap_or_default();
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::Diagnostics {
            unit_id,
            sub_function,
            data,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }

    /// Reads device identification information (MEI Function Code 43, Sub-code 14).
    ///
    /// This allows reading standard device information like Vendor Name, Product Code, etc.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} [options.readDeviceIdCode=1] - The type of read (1=Basic, 2=Regular, 3=Extended).
    /// @param {number} [options.objectId=0] - The specific object ID to start reading from (0-255).
    /// @returns {Promise<object>} A promise that resolves to an object containing the device identification data.
    ///
    /// @example
    /// ```javascript
    /// const id = await client.readDeviceIdentification({
    ///   readDeviceIdCode: 1, // Basic device identification
    ///   objectId: 0,
    /// });
    ///
    /// // id.objects will be an array like:
    /// // [{ id: 0, value: "VendorName" }, { id: 1, value: "ProductCode" }]
    /// ```
    #[wasm_bindgen(js_name = "readDeviceIdentification", skip_typescript)]
    #[cfg(feature = "diagnostics")]
    pub fn read_device_identification(
        &mut self,
        options: ReadDeviceIdentificationOptions,
    ) -> Promise {
        let read_device_id_code = options.read_device_id_code.unwrap_or(1);
        let object_id = options.object_id.unwrap_or(0);
        let signal = options.signal;
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadDeviceIdentification {
            unit_id,
            read_device_id_code,
            object_id,
            resp: tx,
        };
        self.dispatch(cmd, rx, &signal)
    }
}

// ── Promise constructor helper ────────────────────────────────────────────────

fn make_promise() -> (Promise, Function, Function) {
    let resolve_holder: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));
    let reject_holder: Rc<RefCell<Option<Function>>> = Rc::new(RefCell::new(None));

    let r = resolve_holder.clone();
    let rj = reject_holder.clone();

    let promise = Promise::new(&mut move |res, rej| {
        *r.borrow_mut() = Some(res);
        *rj.borrow_mut() = Some(rej);
    });

    let resolve = resolve_holder.borrow_mut().take().unwrap();
    let reject = reject_holder.borrow_mut().take().unwrap();

    (promise, resolve, reject)
}
