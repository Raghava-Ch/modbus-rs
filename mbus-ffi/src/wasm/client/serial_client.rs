//! Browser Web Serial support for WASM.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use futures_util::FutureExt;
use js_sys::{Array, Function, Promise, Reflect};
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, TransportType, UnitIdOrSlaveAddr,
};
use mbus_serial::{WasmAsciiTransport, WasmRtuTransport};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::command::WasmCommand;
use super::helpers::*;
use super::net_client::CreateClientOptions;
use super::response::WasmResponse;
use super::task::{WasmAsyncTransportTrait, WasmClientTask};

#[cfg(feature = "file-record")]
use mbus_client::services::file_record::SubRequestParams;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface WasmSerialTransportOptions {
  /**
   * The serial mode to use.
   * @default "rtu"
   */
  mode?: "rtu" | "ascii";
  /** @default 9600 */
  baudRate?: number;
  /** @default 8 */
  dataBits?: 7 | 8;
  /** @default 1 */
  stopBits?: 1 | 2;
  /** @default "none" */
  parity?: "none" | "even" | "odd";
  /**
   * The maximum time in milliseconds to wait for a response.
   * @default 1000
   */
  responseTimeoutMs?: number;
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
    Rtu(WasmRtuTransport),
    Ascii(WasmAsciiTransport),
}

impl WasmRuntimeSerialTransport {
    fn new(mode: SerialMode) -> Self {
        match mode {
            SerialMode::Rtu => Self::Rtu(WasmRtuTransport::new()),
            SerialMode::Ascii => Self::Ascii(WasmAsciiTransport::new()),
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
#[wasm_bindgen]
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

// ── WasmSerialTransport ──────────────────────────────────────────────────────

#[wasm_bindgen]
/// Connection manager for browser Modbus Serial (RTU/ASCII) clients using the Web Serial API.
///
/// This class manages a single physical serial port connection and multiplexes requests
/// from multiple `WasmSerialModbusClient` instances, which are bound to specific unit IDs
/// (slave addresses) on the same serial bus.
pub struct WasmSerialTransport {
    _port_handle: WasmSerialPortHandle,
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    pending_count: Rc<Cell<usize>>,
    active_transport: Rc<RefCell<Option<WasmRuntimeSerialTransport>>>,
    response_timeout_ms: u32,
}

#[wasm_bindgen]
impl WasmSerialTransport {
    /// Creates and opens a new Serial transport using the provided port handle and options.
    ///
    /// The constructor immediately attempts to open the serial port with the specified parameters.
    ///
    /// @param {WasmSerialPortHandle} port_handle - The opaque handle obtained from `request_serial_port()`.
    /// @param {WasmSerialTransportOptions} [options] - Configuration for the serial connection.
    /// @returns {WasmSerialTransport} A new transport instance.
    /// @throws {Error} If the port fails to open or parameters are invalid.
    ///
    /// @example
    /// ```javascript
    /// // Assumes `portHandle` is an existing WasmSerialPortHandle.
    /// const transport = new WasmSerialTransport(portHandle, { mode: 'rtu', baudRate: 19200 });
    /// const client = transport.createClient({ unitId: 5 });
    /// ```
    #[wasm_bindgen(constructor)]
    pub fn new(
        port_handle: WasmSerialPortHandle,
        options: Option<WasmSerialTransportOptions>,
    ) -> Result<WasmSerialTransport, JsValue> {
        let options_val = options.map(JsValue::from).unwrap_or(JsValue::UNDEFINED);
        let mode_str = get_string(&options_val, "mode", "rtu");
        let baud_rate = get_u32(&options_val, "baudRate", 9600);
        let data_bits = get_u8(&options_val, "dataBits", 8);
        let stop_bits = get_u8(&options_val, "stopBits", 1);
        let parity_str = get_string(&options_val, "parity", "none");
        let response_timeout_ms = get_u32(&options_val, "responseTimeoutMs", 1000);
        let retry_attempts = get_u8(&options_val, "retryAttempts", 0);

        let mode = if mode_str == "ascii" {
            SerialMode::Ascii
        } else {
            SerialMode::Rtu
        };

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
            retry_attempts,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let mut transport = WasmRuntimeSerialTransport::new(mode);
        transport.attach_port(port_handle.clone_port());

        // Sync connect
        transport
            .connect(&config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let (cmd_tx, cmd_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
        let pending_count = Rc::new(Cell::new(0));
        let active_transport = Rc::new(RefCell::new(Some(transport)));

        // Spawn loop
        let active_transport_clone = active_transport.clone();
        let ttype = TransportType::CustomSerial(mode);
        spawn_local(async move {
            let transport_opt = active_transport_clone.borrow_mut().take();
            if let Some(t) = transport_opt {
                let task = WasmClientTask::new(t, cmd_rx, ttype);
                task.run().await;
            }
        });

        Ok(WasmSerialTransport {
            _port_handle: port_handle,
            cmd_tx: Rc::new(RefCell::new(cmd_tx)),
            pending_count,
            active_transport,
            response_timeout_ms,
        })
    }

    /// Returns `true` if there are any in-flight Modbus requests pending a response.
    #[wasm_bindgen(getter, js_name = "pendingRequests")]
    pub fn pending_requests(&self) -> bool {
        self.pending_count.get() > 0
    }

    /// Closes the serial port connection and terminates the background task.
    pub fn close(&mut self) {
        // Drop the command sender to terminate the background task.
        *self.cmd_tx.borrow_mut() = futures_channel::mpsc::unbounded::<WasmCommand>().0;
        self.pending_count.set(0);
        // Attempt to gracefully close the underlying serial port.
        if let Some(t) = self.active_transport.borrow_mut().as_mut() {
            let _ = t.disconnect();
        }
        *self.active_transport.borrow_mut() = None;
    }

    /// Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
    ///
    /// Multiple clients for different unit IDs can be created from a single transport.
    /// They all share the same underlying serial port connection.
    ///
    /// @param {CreateClientOptions} options - The client configuration.
    /// @returns {WasmSerialModbusClient} A new client instance.
    /// @throws {Error} If `options.unitId` is missing or invalid.
    ///
    /// @example
    /// ```javascript
    /// // Assumes `transport` is an existing WasmSerialTransport instance.
    ///
    /// // Create a client for the device at unit ID 1
    /// const client1 = transport.createClient({ unitId: 1 });
    ///
    /// // Create another client for a different device on the same RS-485 bus
    /// const client2 = transport.createClient({ unitId: 10 });
    /// ```
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
            response_timeout_ms: self.response_timeout_ms,
        })
    }
}

// ── WasmSerialModbusClient ───────────────────────────────────────────────────

#[wasm_bindgen]
/// A browser-facing Modbus serial client bound to a specific unit ID (slave address).
///
/// This class provides methods for all standard Modbus function codes. All operations
/// are asynchronous and return a `Promise`. It is created via `WasmSerialTransport.createClient()`.
pub struct WasmSerialModbusClient {
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    unit_id: u8,
    pending_count: Rc<Cell<usize>>,
    response_timeout_ms: u32,
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
    ) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let pending_count = self.pending_count.clone();
        pending_count.set(pending_count.get() + 1);

        if self.cmd_tx.borrow().unbounded_send(cmd).is_err() {
            pending_count.set(pending_count.get() - 1);
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("ConnectionClosed"));
            return promise;
        }

        let timeout_ms = self.response_timeout_ms;

        spawn_local(async move {
            let timeout_fut = gloo_timers::future::TimeoutFuture::new(timeout_ms);
            futures_util::select! {
                res = rx.fuse() => {
                    pending_count.set(pending_count.get() - 1);
                    match res {
                        Ok(Ok(resp)) => {
                            let _ = resolve.call1(&JsValue::NULL, &resp.to_js_value());
                        }
                        Ok(Err(err)) => {
                            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&err));
                        }
                        Err(_) => {
                            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("ConnectionLost"));
                        }
                    }
                }
                _ = timeout_fut.fuse() => {
                    pending_count.set(pending_count.get() - 1);
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("Timeout"));
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
    #[wasm_bindgen(js_name = "readCoils")]
    pub fn read_coils(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let quantity = get_u16(options, "quantity", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadCoils {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "writeSingleCoil")]
    pub fn write_single_coil(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let value = get_bool(options, "value", false);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteSingleCoil {
            unit_id,
            address,
            value,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "writeMultipleCoils")]
    pub fn write_multiple_coils(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let values = match get_bool_array(options, "values") {
            Ok(v) => v,
            Err(e) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&e));
                return promise;
            }
        };
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteMultipleCoils {
            unit_id,
            address,
            values,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readDiscreteInputs")]
    pub fn read_discrete_inputs(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let quantity = get_u16(options, "quantity", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadDiscreteInputs {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readHoldingRegisters")]
    pub fn read_holding_registers(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let quantity = get_u16(options, "quantity", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadHoldingRegisters {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readInputRegisters")]
    pub fn read_input_registers(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let quantity = get_u16(options, "quantity", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadInputRegisters {
            unit_id,
            address,
            quantity,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "writeSingleRegister")]
    pub fn write_single_register(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let value = get_u16(options, "value", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteSingleRegister {
            unit_id,
            address,
            value,
            resp: tx,
        };
        self.dispatch(cmd, rx)
    }

    /// Writes a sequence of holding registers (Function Code 16).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.address - The starting address of the registers to write (0-based).
    /// @param {number[] | Uint16Array} options.values - An array of 16-bit values to write.
    /// @returns {Promise<void>} A promise that resolves when the write is complete.
    ///
    /// @example
    /// ```javascript
    /// await client.writeMultipleRegisters({ address: 200, values: });
    /// await client.writeMultipleRegisters({ address: 210, values: Uint16Array.from() });
    /// ```
    #[wasm_bindgen(js_name = "writeMultipleRegisters")]
    pub fn write_multiple_registers(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let values = match get_u16_array(options, "values") {
            Ok(v) => v,
            Err(e) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&e));
                return promise;
            }
        };
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteMultipleRegisters {
            unit_id,
            address,
            values,
            resp: tx,
        };
        self.dispatch(cmd, rx)
    }

    /// Performs an atomic read and write of holding registers in a single transaction (Function Code 23).
    ///
    /// The write operation is performed before the read.
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.readAddress - The starting address for the read operation.
    /// @param {number} options.readQuantity - The number of registers to read.
    /// @param {number} options.writeAddress - The starting address for the write operation.
    /// @param {number[] | Uint16Array} options.writeValues - The values to write.
    /// @returns {Promise<Uint16Array>} A promise that resolves to a `Uint16Array` of the registers read.
    ///
    /// @example
    /// ```javascript
    /// const readData = await client.readWriteMultipleRegisters({
    ///   readAddress: 10, readQuantity: 2, writeAddress: 20, writeValues:
    /// });
    /// ```
    #[wasm_bindgen(js_name = "readWriteMultipleRegisters")]
    pub fn read_write_multiple_registers(&mut self, options: &JsValue) -> Promise {
        let read_address = get_u16(options, "readAddress", 0);
        let read_quantity = get_u16(options, "readQuantity", 0);
        let write_address = get_u16(options, "writeAddress", 0);
        let write_values = match get_u16_array(options, "writeValues") {
            Ok(v) => v,
            Err(e) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&e));
                return promise;
            }
        };
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
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "maskWriteRegister")]
    pub fn mask_write_register(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let and_mask = get_u16(options, "andMask", 0xFFFF);
        let or_mask = get_u16(options, "orMask", 0x0000);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::MaskWriteRegister {
            unit_id,
            address,
            and_mask,
            or_mask,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readFifoQueue")]
    #[cfg(feature = "fifo")]
    pub fn read_fifo_queue(&mut self, options: &JsValue) -> Promise {
        let address = get_u16(options, "address", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadFifoQueue {
            unit_id,
            address,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readFileRecord")]
    #[cfg(feature = "file-record")]
    pub fn read_file_record(&mut self, options: &JsValue) -> Promise {
        let reqs_val = match Reflect::get(options, &JsValue::from_str("requests")) {
            Ok(v) => v,
            Err(_) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(
                    &JsValue::NULL,
                    &JsValue::from_str("Missing property 'requests'"),
                );
                return promise;
            }
        };
        if !Array::is_array(&reqs_val) {
            let (promise, _, reject) = make_promise();
            let _ = reject.call1(
                &JsValue::NULL,
                &JsValue::from_str("Property 'requests' must be an array"),
            );
            return promise;
        }
        let arr = Array::from(&reqs_val);
        let mut requests = Vec::new();
        for i in 0..arr.length() {
            let item = arr.get(i);
            let file_number = get_u16(&item, "fileNumber", 0);
            let record_number = get_u16(&item, "recordNumber", 0);
            let record_length = get_u16(&item, "recordLength", 0);
            requests.push(SubRequestParams {
                file_number,
                record_number,
                record_length,
                record_data: None,
            });
        }

        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadFileRecord {
            unit_id,
            requests,
            resp: tx,
        };
        self.dispatch(cmd, rx)
    }

    /// Writes one or more file records (Function Code 15).
    ///
    /// @param {object} options - The request parameters.
    /// @param {object[]} options.requests - An array of sub-request objects to write.
    /// @param {number} options.requests[].fileNumber - The file number.
    /// @param {number} options.requests[].recordNumber - The starting record number within the file.
    /// @param {number[] | Uint16Array} options.requests[].recordData - The register data to write.
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
    #[wasm_bindgen(js_name = "writeFileRecord")]
    #[cfg(feature = "file-record")]
    pub fn write_file_record(&mut self, options: &JsValue) -> Promise {
        let reqs_val = match Reflect::get(options, &JsValue::from_str("requests")) {
            Ok(v) => v,
            Err(_) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(
                    &JsValue::NULL,
                    &JsValue::from_str("Missing property 'requests'"),
                );
                return promise;
            }
        };
        if !Array::is_array(&reqs_val) {
            let (promise, _, reject) = make_promise();
            let _ = reject.call1(
                &JsValue::NULL,
                &JsValue::from_str("Property 'requests' must be an array"),
            );
            return promise;
        }
        let arr = Array::from(&reqs_val);
        let mut requests = Vec::new();
        for i in 0..arr.length() {
            let item = arr.get(i);
            let file_number = get_u16(&item, "fileNumber", 0);
            let record_number = get_u16(&item, "recordNumber", 0);
            let record_data_val = match get_u16_array(&item, "recordData") {
                Ok(d) => d,
                Err(e) => {
                    let (promise, _, reject) = make_promise();
                    let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&e));
                    return promise;
                }
            };
            let mut record_length = get_u16(&item, "recordLength", 0);
            if record_length == 0 {
                record_length = record_data_val.len() as u16;
            }
            let mut hv_data = heapless::Vec::new();
            if hv_data.extend_from_slice(&record_data_val).is_err() {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(
                    &JsValue::NULL,
                    &JsValue::from_str("Too many registers in recordData"),
                );
                return promise;
            }
            requests.push(SubRequestParams {
                file_number,
                record_number,
                record_length,
                record_data: Some(hv_data),
            });
        }

        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::WriteFileRecord {
            unit_id,
            requests,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readExceptionStatus")]
    #[cfg(feature = "diagnostics")]
    pub fn read_exception_status(&mut self) -> Promise {
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadExceptionStatus { unit_id, resp: tx };
        self.dispatch(cmd, rx)
    }

    /// Performs a diagnostic function on the device (Function Code 08).
    ///
    /// @param {object} options - The request parameters.
    /// @param {number} options.subFunction - The diagnostic sub-function code to execute.
    /// @param {number[] | Uint16Array} [options.data] - Optional data to send with the request.
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
    #[wasm_bindgen(js_name = "diagnostics")]
    #[cfg(feature = "diagnostics")]
    pub fn diagnostics(&mut self, options: &JsValue) -> Promise {
        let sub_function = get_u16(options, "subFunction", 0);
        let data = match get_u16_array(options, "data") {
            Ok(d) => d,
            Err(e) => {
                let (promise, _, reject) = make_promise();
                let _ = reject.call1(&JsValue::NULL, &JsValue::from_str(&e));
                return promise;
            }
        };
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::Diagnostics {
            unit_id,
            sub_function,
            data,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
    #[wasm_bindgen(js_name = "readDeviceIdentification")]
    #[cfg(feature = "diagnostics")]
    pub fn read_device_identification(&mut self, options: &JsValue) -> Promise {
        let read_device_id_code = get_u8(options, "readDeviceIdCode", 1);
        let object_id = get_u8(options, "objectId", 0);
        let (tx, rx) = futures_channel::oneshot::channel();
        let unit_id = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let cmd = WasmCommand::ReadDeviceIdentification {
            unit_id,
            read_device_id_code,
            object_id,
            resp: tx,
        };
        self.dispatch(cmd, rx)
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
