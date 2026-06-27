//! `WasmModbusClient` and `WasmTcpTransport` — `#[wasm_bindgen]` async entry points.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use futures_util::FutureExt;
use js_sys::{Array, Function, Promise, Reflect};
use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::command::WasmCommand;
use super::helpers::*;
use super::response::WasmResponse;
use super::task::WasmClientTask;
use mbus_network::WasmAsyncTransport;

#[cfg(feature = "file-record")]
use mbus_client::services::file_record::SubRequestParams;

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND_CONTENT: &'static str = r#"
export interface WasmTcpTransportOptions {
  /**
   * The maximum time in milliseconds to wait for a response from the Modbus device.
   * @default 3000
   */
  responseTimeoutMs?: number;
}
export interface CreateClientOptions {
  unitId: number;
}"#;

#[wasm_bindgen]
extern "C" {
    /// Options for creating a `WasmTcpTransport`.
    ///
    /// ```typescript
    /// interface WasmTcpTransportOptions {
    ///   /**
    ///    * The maximum time in milliseconds to wait for a response from the Modbus device.
    ///    * @default 3000
    ///    */
    ///   responseTimeoutMs?: number;
    /// }
    /// ```
    #[wasm_bindgen(typescript_type = "WasmTcpTransportOptions")]
    pub type WasmTcpTransportOptions;

    /// Options for creating a `WasmModbusClient` instance.
    #[wasm_bindgen(typescript_type = "CreateClientOptions")]
    pub type CreateClientOptions;
}

// ── WasmTcpTransport ─────────────────────────────────────────────────────────

#[wasm_bindgen]
/// Connection manager for browser Modbus TCP clients, communicating over a WebSocket gateway.
///
/// This class manages a single WebSocket connection and multiplexes requests from multiple
/// `WasmModbusClient` instances, which are bound to specific unit IDs. It handles the
/// connection lifecycle, request dispatch, and response routing.
pub struct WasmTcpTransport {
    ws_url: String,
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    pending_count: Rc<Cell<usize>>,
    active_transport: Rc<RefCell<Option<WasmAsyncTransport>>>,
    response_timeout_ms: u32,
}

#[wasm_bindgen]
impl WasmTcpTransport {
    /// Establishes a connection to a Modbus TCP server via a WebSocket gateway.
    ///
    /// This is the entry point for creating a new TCP transport. It returns a `Promise`
    /// that resolves to a `WasmTcpTransport` instance upon a successful WebSocket
    /// connection, or rejects if the connection fails.
    ///
    /// @param {string} ws_url - The URL of the WebSocket gateway (e.g., "ws://127.0.0.1:8080").
    /// @param {WasmTcpTransportOptions} [options] - Optional connection parameters.
    /// @returns {Promise<WasmTcpTransport>} A promise that resolves with the transport instance.
    ///
    /// @example
    /// ```javascript
    /// const transport = await WasmTcpTransport.connect("ws://localhost:8080", { responseTimeoutMs: 2000 });
    /// ```
    #[wasm_bindgen(js_name = "connect")]
    pub fn connect(ws_url: &str, options: Option<WasmTcpTransportOptions>) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let ws_url_str = ws_url.to_string();
        let options_val = options.map(JsValue::from).unwrap_or(JsValue::UNDEFINED);

        spawn_local(async move {
            match Self::connect_rust(&ws_url_str, &options_val).await {
                Ok(js_transport) => {
                    let _ = resolve.call1(&JsValue::NULL, &js_transport.into());
                }
                Err(err) => {
                    let _ = reject.call1(&JsValue::NULL, &err);
                }
            }
        });

        promise
    }

    /// Rust-internal async connect helper
    pub async fn connect_rust(
        ws_url: &str,
        options: &JsValue,
    ) -> Result<WasmTcpTransport, JsValue> {
        let transport = WasmAsyncTransport::connect(ws_url).await?;
        let (cmd_tx, cmd_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
        let pending_count = Rc::new(Cell::new(0));
        let active_transport = Rc::new(RefCell::new(Some(transport)));
        let response_timeout_ms = get_u32(options, "responseTimeoutMs", 3000);

        // Spawn the message task loop
        let active_transport_clone = active_transport.clone();
        spawn_local(async move {
            let transport_opt = active_transport_clone.borrow_mut().take();
            if let Some(t) = transport_opt {
                let task = WasmClientTask::new(t, cmd_rx, TransportType::CustomTcp);
                task.run().await;
            }
        });

        Ok(WasmTcpTransport {
            ws_url: ws_url.to_string(),
            cmd_tx: Rc::new(RefCell::new(cmd_tx)),
            pending_count,
            active_transport,
            response_timeout_ms,
        })
    }

    /// Drop all pending in-flight requests and attempt to reconnect.
    ///
    /// This method closes the current WebSocket connection, discards any outstanding
    /// requests, and establishes a new connection to the same URL. It returns a `Promise`
    /// that resolves when the reconnection is successful.
    ///
    /// @returns {Promise<void>}
    pub fn reconnect(&mut self) -> Promise {
        let (promise, resolve, reject) = make_promise();
        let ws_url_str = self.ws_url.clone();
        let cmd_tx_cell = self.cmd_tx.clone();
        let pending_count_cell = self.pending_count.clone();
        let active_transport_cell = self.active_transport.clone();

        spawn_local(async move {
            // Reconnect WebSocket
            match WasmAsyncTransport::connect(&ws_url_str).await {
                Ok(transport) => {
                    // Create new command channel
                    let (new_tx, new_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();

                    // Replace current command channel sender
                    *cmd_tx_cell.borrow_mut() = new_tx;

                    // Set transport and spawn new task
                    *active_transport_cell.borrow_mut() = Some(transport);
                    let active_transport_clone = active_transport_cell.clone();
                    spawn_local(async move {
                        let transport_opt = active_transport_clone.borrow_mut().take();
                        if let Some(t) = transport_opt {
                            let task = WasmClientTask::new(t, new_rx, TransportType::CustomTcp);
                            task.run().await;
                        }
                    });

                    // Reset pending count
                    pending_count_cell.set(0);

                    let _ = resolve.call0(&JsValue::NULL);
                }
                Err(err) => {
                    let _ = reject.call1(&JsValue::NULL, &err);
                }
            }
        });

        promise
    }

    /// Rust-internal async reconnect helper
    pub async fn reconnect_rust(&mut self) -> Result<(), JsValue> {
        let ws_url_str = self.ws_url.clone();
        let transport = WasmAsyncTransport::connect(&ws_url_str).await?;

        let (new_tx, new_rx) = futures_channel::mpsc::unbounded::<WasmCommand>();
        *self.cmd_tx.borrow_mut() = new_tx;
        *self.active_transport.borrow_mut() = Some(transport);

        let active_transport_clone = self.active_transport.clone();
        spawn_local(async move {
            let transport_opt = active_transport_clone.borrow_mut().take();
            if let Some(t) = transport_opt {
                let task = WasmClientTask::new(t, new_rx, TransportType::CustomTcp);
                task.run().await;
            }
        });

        self.pending_count.set(0);
        Ok(())
    }

    /// Closes the WebSocket connection and terminates the background task.
    ///
    /// All subsequent requests on clients created from this transport will fail.
    pub fn close(&mut self) {
        // Drop the cmd_tx sender to terminate the task
        *self.cmd_tx.borrow_mut() = futures_channel::mpsc::unbounded::<WasmCommand>().0;
        self.pending_count.set(0);
        *self.active_transport.borrow_mut() = None;
    }

    /// Returns `true` if there are any in-flight Modbus requests pending a response.
    #[wasm_bindgen(getter, js_name = "pendingRequests")]
    pub fn pending_requests(&self) -> bool {
        self.pending_count.get() > 0
    }

    /// Creates a lightweight client instance bound to a specific Modbus unit ID (slave address).
    ///
    /// Multiple clients can be created from a single transport. They all share the same
    //  underlying WebSocket connection.
    ///
    /// @param {CreateClientOptions} options - The client configuration.
    /// @returns {WasmModbusClient} A new client instance.
    /// @throws {Error} If `options.unitId` is missing or invalid.
    ///
    /// @example
    /// ```javascript
    /// // Assumes `transport` is an existing WasmTcpTransport instance.
    ///
    /// // Create a client for the device at unit ID 1
    /// const client1 = transport.createClient({ unitId: 1 });
    ///
    /// // Create another client for a different device on the same bus
    /// const client2 = transport.createClient({ unitId: 10 });
    /// ```
    #[wasm_bindgen(js_name = "createClient")]
    pub fn create_client(&self, options: CreateClientOptions) -> Result<WasmModbusClient, JsValue> {
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

        Ok(WasmModbusClient {
            cmd_tx: self.cmd_tx.clone(),
            unit_id,
            pending_count: self.pending_count.clone(),
            response_timeout_ms: self.response_timeout_ms,
        })
    }
}

// ── WasmModbusClient ──────────────────────────────────────────────────────────

#[wasm_bindgen]
/// A browser-facing Modbus client bound to a specific unit ID (slave address).
///
/// This class provides methods for all standard Modbus function codes. All operations
/// are asynchronous and return a `Promise`.
pub struct WasmModbusClient {
    cmd_tx: Rc<RefCell<futures_channel::mpsc::UnboundedSender<WasmCommand>>>,
    unit_id: u8,
    pending_count: Rc<Cell<usize>>,
    response_timeout_ms: u32,
}

#[wasm_bindgen]
impl WasmModbusClient {
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
    /// console.log(regs); // Access the first register value
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
    ///     { fileNumber: 4, recordNumber: 1, recordLength: 2 }
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
    ///     { fileNumber: 4, recordNumber: 1, recordData: }
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

/// Returns a `(Promise, resolve, reject)` tuple. The caller can spawn the task in the background
/// and use `resolve` or `reject` to complete the promise when the background task finishes or fails.
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
