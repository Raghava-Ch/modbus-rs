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
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    Parity, SerialMode, UnitIdOrSlaveAddr,
};
use mbus_serial::WasmSerialTransport;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::{JsFuture, spawn_local};

use super::app::{PendingHandle, PendingMap, WasmAppRouter};

const PIPELINE: usize = 10;
type Inner = ClientServices<WasmSerialTransport, WasmAppRouter, PIPELINE>;

#[wasm_bindgen]
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

        let mut transport = WasmSerialTransport::new(serial_mode);
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

        spawn_local(async move {
            loop {
                match weak.upgrade() {
                    Some(rc) => rc.borrow_mut().poll(),
                    None => break,
                }
                sleep(Duration::from_millis(tick_ms)).await;
            }
        });

        Ok(WasmSerialModbusClient {
            inner,
            pending,
            unit_id,
            next_txn: 1,
        })
    }

    pub fn is_connected(&self) -> bool {
        self.inner.borrow().is_connected()
    }

    pub fn reconnect(&mut self) -> bool {
        for (_, handle) in self.pending.borrow_mut().drain() {
            let _ = handle
                .reject
                .call1(&JsValue::NULL, &JsValue::from_str("ConnectionLost"));
        }
        self.inner.borrow_mut().reconnect().is_ok()
    }

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
}

impl WasmSerialModbusClient {
    fn alloc_txn(&mut self) -> u16 {
        let id = self.next_txn;
        self.next_txn = self.next_txn.wrapping_add(1).max(1);
        id
    }

    fn reject_immediate(&self, txn_id: u16, error: MbusError) {
        if let Some(handle) = self.pending.borrow_mut().remove(&txn_id) {
            let _ = handle.reject.call1(
                &JsValue::NULL,
                &JsValue::from_str(&format!("{:?}", error)),
            );
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
