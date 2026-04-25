//! Browser Web Serial support for WASM.
//!
//! This module exposes:
//! - `request_serial_port()` (must be called from a user gesture in JS)
//! - `WasmSerialPortHandle` to hold the granted browser `SerialPort`
//! - `WasmSerialModbusClient` that uses `mbus_serial::WasmSerialTransport`

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gloo_timers::future::sleep;
use js_sys::{Function, Promise, Reflect};
use mbus_client::services::ClientServices;
use mbus_client::services::coil::Coils;
#[cfg(feature = "file-record")]
use mbus_client::services::file_record::SubRequest;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
#[cfg(feature = "file-record")]
use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;
use mbus_core::errors::MbusError;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig, Parity,
    SerialMode, UnitIdOrSlaveAddr,
};
use mbus_serial::{WasmAsciiTransport, WasmRtuTransport};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::app::{PendingHandle, PendingMap, WasmAppRouter};

const PIPELINE: usize = 1;

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
}

impl mbus_core::transport::Transport for WasmRuntimeSerialTransport {
    type Error = mbus_core::transport::TransportError;
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: mbus_core::transport::TransportType =
        mbus_core::transport::TransportType::CustomSerial(SerialMode::Rtu);

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        match self {
            Self::Rtu(transport) => transport.connect(config),
            Self::Ascii(transport) => transport.connect(config),
        }
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        match self {
            Self::Rtu(transport) => transport.disconnect(),
            Self::Ascii(transport) => transport.disconnect(),
        }
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        match self {
            Self::Rtu(transport) => transport.send(adu),
            Self::Ascii(transport) => transport.send(adu),
        }
    }

    fn recv(&mut self) -> Result<heapless::Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        match self {
            Self::Rtu(transport) => transport.recv(),
            Self::Ascii(transport) => transport.recv(),
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            Self::Rtu(transport) => transport.is_connected(),
            Self::Ascii(transport) => transport.is_connected(),
        }
    }
}

type Inner = ClientServices<WasmRuntimeSerialTransport, WasmAppRouter, PIPELINE>;

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
    fn clone_port(&self) -> JsValue {
        self.port.clone()
    }
}

impl WasmSerialPortHandle {
    /// Construct a handle wrapping any JS value.
    ///
    /// This is only intended for test code where calling `request_serial_port()` is
    /// not possible (no user gesture). In production JS, use `request_serial_port()`.
    #[doc(hidden)]
    pub fn new_for_testing(port: JsValue) -> Self {
        WasmSerialPortHandle { port }
    }
}

/// Requests a browser serial port from `navigator.serial.requestPort()`.
///
/// Must be invoked from a user-gesture context (e.g. click handler).
#[wasm_bindgen]
pub async fn request_serial_port() -> Result<WasmSerialPortHandle, JsValue> {
    let global = js_sys::global();
    let navigator = Reflect::get(&global, &JsValue::from_str("navigator"))?;
    let serial = Reflect::get(&navigator, &JsValue::from_str("serial"))?;

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

#[wasm_bindgen]
/// Browser-facing Modbus client that communicates over Web Serial RTU or ASCII.
pub struct WasmSerialModbusClient {
    inner: Rc<RefCell<Inner>>,
    pending: PendingMap,
    unit_id: u8,
    next_txn: u16,
}

#[wasm_bindgen]
impl WasmSerialModbusClient {
    /// Creates a Modbus serial client over browser Web Serial.
    ///
    /// `mode` accepts "rtu" or "ascii" (case-insensitive).
    /// `parity` accepts "none", "even", or "odd".
    #[wasm_bindgen(constructor)]
    pub fn new(
        port_handle: &WasmSerialPortHandle,
        unit_id: u8,
        mode: &str,
        baud_rate: u32,
        data_bits: u8,
        stop_bits: u8,
        parity: &str,
        response_timeout_ms: u32,
        retry_attempts: u8,
        tick_interval_ms: u32,
    ) -> Result<WasmSerialModbusClient, JsValue> {
        let serial_mode = match mode.to_ascii_lowercase().as_str() {
            "rtu" => SerialMode::Rtu,
            "ascii" => SerialMode::Ascii,
            _ => return Err(JsValue::from_str("mode must be 'rtu' or 'ascii'")),
        };

        let parity_cfg = match parity.to_ascii_lowercase().as_str() {
            "none" => Parity::None,
            "even" => Parity::Even,
            "odd" => Parity::Odd,
            _ => return Err(JsValue::from_str("parity must be 'none', 'even', or 'odd'")),
        };

        let data_bits_cfg = match data_bits {
            5 => DataBits::Five,
            6 => DataBits::Six,
            7 => DataBits::Seven,
            8 => DataBits::Eight,
            _ => return Err(JsValue::from_str("data_bits must be one of 5, 6, 7, or 8")),
        };

        let mut transport = WasmRuntimeSerialTransport::new(serial_mode);
        transport.attach_port(port_handle.clone_port());

        let pending: PendingMap = Rc::new(RefCell::new(HashMap::new()));
        let app = WasmAppRouter::new(pending.clone());

        let mut port_path = heapless::String::new();
        port_path
            .push_str("web-serial")
            .map_err(|_| JsValue::from_str("failed to build serial port_path"))?;

        let config = ModbusConfig::Serial(ModbusSerialConfig {
            port_path,
            mode: serial_mode,
            baud_rate: match baud_rate {
                9600 => BaudRate::Baud9600,
                19200 => BaudRate::Baud19200,
                _ => BaudRate::Custom(baud_rate),
            },
            data_bits: data_bits_cfg,
            stop_bits,
            parity: parity_cfg,
            response_timeout_ms,
            retry_attempts,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let inner_client = ClientServices::new(transport, app, config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let inner = Rc::new(RefCell::new(inner_client));
        let weak = Rc::downgrade(&inner);
        let tick_ms = tick_interval_ms as u64;
        let idle_ms = core::cmp::max(50, tick_ms.saturating_mul(5));

        spawn_local(async move {
            loop {
                match weak.upgrade() {
                    Some(rc) => {
                        let should_poll = {
                            let client = rc.borrow();
                            client.is_connected() && client.has_pending_requests()
                        };

                        if should_poll {
                            rc.borrow_mut().poll();
                            sleep(Duration::from_millis(tick_ms)).await;
                        } else {
                            sleep(Duration::from_millis(idle_ms)).await;
                        }
                        continue;
                    }
                    None => break,
                }
            }
        });

        Ok(WasmSerialModbusClient {
            inner,
            pending,
            unit_id,
            next_txn: 1,
        })
    }

    /// Returns `true` when the serial port is open and the transport considers itself connected.
    pub fn is_connected(&self) -> bool {
        self.inner.borrow().is_connected()
    }

    /// Returns `true` if there are in-flight Modbus requests waiting for
    /// response/timeout resolution.
    pub fn has_pending_requests(&self) -> bool {
        self.inner.borrow().has_pending_requests()
    }

    /// Drop all pending in-flight requests and attempt to reopen the serial port.
    /// Outstanding Promises for dropped requests will be rejected with `"ConnectionLost"`.
    pub fn reconnect(&mut self) -> bool {
        for (_, handle) in self.pending.borrow_mut().drain() {
            let _ = handle
                .reject
                .call1(&JsValue::NULL, &JsValue::from_str("ConnectionLost"));
        }
        self.inner.borrow_mut().reconnect().is_ok()
    }

    /// Read `quantity` coils starting at `address`.
    ///
    /// Returns a `Promise` resolving with a `Uint8Array` (bit-packed coil bytes) or rejects on error.
    pub fn read_coils(&mut self, address: u16, quantity: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .coils()
            .read_multiple_coils(txn_id, unit_addr, address, quantity);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read `quantity` holding registers starting at `address`.
    ///
    /// Returns a `Promise` resolving with a `Uint16Array` (register values) or rejects on error.
    pub fn read_holding_registers(&mut self, address: u16, quantity: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .read_holding_registers(txn_id, unit_addr, address, quantity);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read `quantity` input registers starting at `address`.
    ///
    /// Returns a `Promise` resolving with a `Uint16Array` or rejects on error.
    pub fn read_input_registers(&mut self, address: u16, quantity: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .read_input_registers(txn_id, unit_addr, address, quantity);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Write `value` to a single holding register at `address`.
    ///
    /// Returns a `Promise` resolving with `{ address, value }` or rejects on error.
    pub fn write_single_register(&mut self, address: u16, value: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .write_single_register(txn_id, unit_addr, address, value);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Write a single coil at `address` to `value` (true = ON, false = OFF).
    ///
    /// Returns a `Promise` resolving with `{ address, value }` or rejects on error.
    pub fn write_single_coil(&mut self, address: u16, value: bool) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .coils()
            .write_single_coil(txn_id, unit_addr, address, value);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Write `values` to multiple consecutive holding registers starting at `address`.
    ///
    /// Returns a `Promise` resolving with `{ address, quantity }` or rejects on error.
    pub fn write_multiple_registers(
        &mut self,
        address: u16,
        quantity: u16,
        values: &[u16],
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .write_multiple_registers(txn_id, unit_addr, address, quantity, values);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read `quantity` discrete inputs starting at `address`.
    ///
    /// Returns a `Promise` resolving with a `Uint8Array` (bit-packed) or rejects on error.
    pub fn read_discrete_inputs(&mut self, address: u16, quantity: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .discrete_inputs()
            .read_discrete_inputs(txn_id, unit_addr, address, quantity);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }
}

#[wasm_bindgen]
impl WasmSerialModbusClient {
    /// Read a single coil at `address`.
    ///
    /// Returns a `Promise` resolving with a `boolean` or rejects on error.
    pub fn read_single_coil(&mut self, address: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .coils()
            .read_single_coil(txn_id, unit_addr, address);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Write multiple coils starting at `address`.
    ///
    /// `packed_bytes` is a bit-packed `Uint8Array` (LSB of byte 0 = coil at `address`).
    /// Returns a `Promise` resolving with `{ address, quantity }` or rejects on error.
    pub fn write_multiple_coils(
        &mut self,
        address: u16,
        quantity: u16,
        packed_bytes: &[u8],
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let coils_result =
            Coils::new(address, quantity).and_then(|c| c.with_values(packed_bytes, quantity));
        let result = match coils_result {
            Ok(coils) => self
                .inner
                .borrow_mut()
                .coils()
                .write_multiple_coils(txn_id, unit_addr, address, &coils),
            Err(e) => Err(e),
        };

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read a single holding register at `address`.
    ///
    /// Returns a `Promise` resolving with a `number` or rejects on error.
    pub fn read_single_holding_register(&mut self, address: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .read_single_holding_register(txn_id, unit_addr, address);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read a single input register at `address`.
    ///
    /// Returns a `Promise` resolving with a `number` or rejects on error.
    pub fn read_single_input_register(&mut self, address: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .read_single_input_register(txn_id, unit_addr, address);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Perform an atomic read-then-write on holding registers.
    ///
    /// Reads `read_quantity` registers from `read_address`, then writes `values` to `write_address`.
    /// `write_quantity` is ignored — the quantity written is derived from `values.length`.
    /// Returns a `Promise` resolving with a `Uint16Array` (the values read) or rejects on error.
    pub fn read_write_multiple_registers(
        &mut self,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        _write_quantity: u16,
        values: &[u16],
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .read_write_multiple_registers(
                txn_id,
                unit_addr,
                read_address,
                read_quantity,
                write_address,
                values,
            );

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Apply an AND/OR mask to a holding register at `address` (FC 22).
    ///
    /// Result register = (current & and_mask) | (or_mask & !and_mask).
    /// Returns a `Promise` resolving with `true` or rejects on error.
    pub fn mask_write_register(&mut self, address: u16, and_mask: u16, or_mask: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .registers()
            .mask_write_register(txn_id, unit_addr, address, and_mask, or_mask);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read a single discrete input at `address`.
    ///
    /// Returns a `Promise` resolving with a `boolean` or rejects on error.
    pub fn read_single_discrete_input(&mut self, address: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .discrete_inputs()
            .read_single_discrete_input(txn_id, unit_addr, address);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read the FIFO queue pointed to by `address` (FC 24).
    ///
    /// Returns a `Promise` resolving with a `Uint16Array` or rejects on error.
    #[cfg(feature = "fifo")]
    pub fn read_fifo_queue(&mut self, address: u16) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .fifo()
            .read_fifo_queue(txn_id, unit_addr, address);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read a file record (FC 20).
    ///
    /// Returns a `Promise` resolving with `Array<{ fileNumber, recordNumber, data: Uint16Array }>`
    /// or rejects on error.
    #[cfg(feature = "file-record")]
    pub fn read_file_record(
        &mut self,
        file_number: u16,
        record_number: u16,
        record_length: u16,
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let mut sub_req = SubRequest::new();
        let result = sub_req
            .add_read_sub_request(file_number, record_number, record_length)
            .and_then(|_| {
                self.inner
                    .borrow_mut()
                    .file_records()
                    .read_file_record(txn_id, unit_addr, &sub_req)
            });

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Write a file record (FC 21).
    ///
    /// `values` is a `Uint16Array` of register values to write.
    /// Returns a `Promise` resolving with `true` or rejects on error.
    #[cfg(feature = "file-record")]
    pub fn write_file_record(
        &mut self,
        file_number: u16,
        record_number: u16,
        values: &[u16],
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let record_length = values.len() as u16;

        let mut hv = heapless::Vec::<u16, MAX_PDU_DATA_LEN>::new();
        for &v in values {
            if hv.push(v).is_err() {
                self.reject_immediate(txn_id, MbusError::BufferTooSmall);
                return promise;
            }
        }

        let mut sub_req = SubRequest::new();
        let result = sub_req
            .add_write_sub_request(file_number, record_number, record_length, hv)
            .and_then(|_| {
                self.inner
                    .borrow_mut()
                    .file_records()
                    .write_file_record(txn_id, unit_addr, &sub_req)
            });

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read the exception status (FC 7).
    ///
    /// Returns a `Promise` resolving with a status `number` or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn read_exception_status(&mut self) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .diagnostic()
            .read_exception_status(txn_id, unit_addr);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Send a Diagnostics request (FC 8).
    ///
    /// `sub_function` is one of the `DiagnosticSubFunction` u16 codes.
    /// Returns a `Promise` resolving with `{ subFunction, data: Uint16Array }` or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn diagnostics(&mut self, sub_function: u16, data: &[u16]) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = DiagnosticSubFunction::try_from(sub_function)
            .map_err(|_| MbusError::ReservedSubFunction(sub_function))
            .and_then(|sf| {
                self.inner
                    .borrow_mut()
                    .diagnostic()
                    .diagnostics(txn_id, unit_addr, sf, data)
            });

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read the communication event counter (FC 11).
    ///
    /// Returns a `Promise` resolving with `{ status, eventCount }` or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn get_comm_event_counter(&mut self) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .diagnostic()
            .get_comm_event_counter(txn_id, unit_addr);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read the communication event log (FC 12).
    ///
    /// Returns a `Promise` resolving with `{ status, eventCount, messageCount, events: Uint8Array }`
    /// or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn get_comm_event_log(&mut self) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .diagnostic()
            .get_comm_event_log(txn_id, unit_addr);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Report Server ID (FC 17).
    ///
    /// Returns a `Promise` resolving with a `Uint8Array` (raw server ID data) or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn report_server_id(&mut self) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = self
            .inner
            .borrow_mut()
            .diagnostic()
            .report_server_id(txn_id, unit_addr);

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }

    /// Read Device Identification (FC 43 / MEI 0x0E).
    ///
    /// `read_device_id_code`: 1=Basic, 2=Regular, 3=Extended, 4=Specific.
    /// `object_id`: 0x00=VendorName, 0x01=ProductCode, 0x02=Revision, etc.
    /// Returns a `Promise` resolving with `{ readDeviceIdCode, conformityLevel, moreFollows, objects }`
    /// or rejects on error.
    #[cfg(feature = "diagnostics")]
    pub fn read_device_identification(
        &mut self,
        read_device_id_code: u8,
        object_id: u8,
    ) -> Promise {
        let txn_id = self.alloc_txn();
        let (promise, resolve, reject) = make_promise();
        self.pending
            .borrow_mut()
            .insert(txn_id, PendingHandle { resolve, reject });

        let unit_addr = UnitIdOrSlaveAddr::new(self.unit_id).unwrap_or_default();
        let result = ReadDeviceIdCode::try_from(read_device_id_code)
            .map_err(|_| MbusError::InvalidDeviceIdCode)
            .and_then(|code| {
                self.inner
                    .borrow_mut()
                    .diagnostic()
                    .read_device_identification(txn_id, unit_addr, code, ObjectId::from(object_id))
            });

        if let Err(e) = result {
            self.reject_immediate(txn_id, e);
        }
        promise
    }
}

impl WasmSerialModbusClient {
    fn alloc_txn(&mut self) -> u16 {
        let id = self.next_txn;
        self.next_txn = self.next_txn.wrapping_add(1).max(1);
        id
    }

    fn reject_immediate(&self, txn_id: u16, error: MbusError) {
        if let Some(handle) = self.pending.borrow_mut().remove(&txn_id) {
            let _ = handle
                .reject
                .call1(&JsValue::NULL, &JsValue::from_str(&format!("{:?}", error)));
        }
    }
}

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
