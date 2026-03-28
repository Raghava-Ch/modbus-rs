//! `WasmModbusClient` — `#[wasm_bindgen]` public entry point.
//!
//! Wraps `ClientServices<WasmWsTransport, WasmAppRouter, 10>` behind an `Rc<RefCell<...>>`
//! so it can be shared with the hidden background `spawn_local` tick loop.
//!
//! ## Tick loop
//! The constructor spawns a non-blocking async loop via `wasm_bindgen_futures::spawn_local`.
//! It sleeps for `tick_interval_ms` milliseconds between each `poll()` call using
//! `gloo_timers::future::sleep`. The loop exits automatically when the `Rc` is dropped
//! (i.e. when the JS `WasmModbusClient` instance goes out of scope or is GC'd).
//!
//! ## Promise model
//! Each request method creates a JS `Promise`, stores the (resolve, reject) pair keyed by
//! `txn_id` in the shared `pending` map, enqueues the Modbus request, and returns the
//! `Promise` to JS. When `poll()` receives the response (or times out), `WasmAppRouter`
//! resolves or rejects the promise directly.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Duration;

use gloo_timers::future::sleep;
use js_sys::{Function, Promise};
use mbus_client::services::ClientServices;
use mbus_client::services::coil::Coils;
use mbus_client::services::file_record::SubRequest;
use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::DiagnosticSubFunction;
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
use mbus_core::transport::{
    BackoffStrategy, JitterStrategy, ModbusConfig, ModbusTcpConfig, UnitIdOrSlaveAddr,
};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::spawn_local;

use super::app::{PendingHandle, PendingMap, WasmAppRouter};
use mbus_network::WasmWsTransport;

// Pipeline depth: up to 10 concurrent in-flight TCP requests.
const PIPELINE: usize = 10;

type Inner = ClientServices<WasmWsTransport, WasmAppRouter, PIPELINE>;

// ── WasmModbusClient ──────────────────────────────────────────────────────────

#[wasm_bindgen]
pub struct WasmModbusClient {
    inner: Rc<RefCell<Inner>>,
    pending: PendingMap,
    unit_id: u8,
    /// Monotonically increasing transaction ID counter.
    next_txn: u16,
}

#[wasm_bindgen]
impl WasmModbusClient {
    // ── Constructor ───────────────────────────────────────────────────────────

    /// Create a new Modbus master client and immediately start the background tick loop.
    ///
    /// # Arguments
    /// - `ws_url`           — WebSocket URL of the Modbus/TCP gateway (e.g. `"ws://192.168.1.1:8502"`).
    /// - `unit_id`          — Modbus unit ID / slave address of the target device (1–247).
    /// - `response_timeout_ms` — How long (ms) to wait before retrying or failing a request.
    /// - `retry_attempts`   — Number of retries before reporting an error to JS.
    /// - `tick_interval_ms` — How often (ms) the tick loop calls `poll()`. 20 ms is a safe default.
    #[wasm_bindgen(constructor)]
    pub fn new(
        ws_url: &str,
        unit_id: u8,
        response_timeout_ms: u32,
        retry_attempts: u8,
        tick_interval_ms: u32,
    ) -> Result<WasmModbusClient, JsValue> {
        let pending: PendingMap = Rc::new(RefCell::new(HashMap::new()));
        let app = WasmAppRouter::new(pending.clone());
        let transport = WasmWsTransport::new(ws_url);

        let config = ModbusConfig::Tcp(ModbusTcpConfig {
            host: heapless::String::try_from("wasm")
                .map_err(|_| JsValue::from_str("host string overflow"))?,
            port: 0,
            connection_timeout_ms: 5000,
            response_timeout_ms,
            retry_attempts,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        });

        let inner_client = ClientServices::new(transport, app, config)
            .map_err(|e| JsValue::from_str(&format!("{:?}", e)))?;

        let inner = Rc::new(RefCell::new(inner_client));

        // ── Background tick loop ──────────────────────────────────────────
        // Uses a `Weak` reference so the loop terminates naturally when JS
        // lets the `WasmModbusClient` be garbage collected.
        let weak = Rc::downgrade(&inner);
        let tick_ms = tick_interval_ms as u64;

        spawn_local(async move {
            loop {
                match weak.upgrade() {
                    Some(rc) => rc.borrow_mut().poll(),
                    None => break, // client dropped → stop the loop
                }
                sleep(Duration::from_millis(tick_ms)).await;
            }
        });

        Ok(WasmModbusClient {
            inner,
            pending,
            unit_id,
            next_txn: 1,
        })
    }

    // ── Status ───────────────────────────────────────────────────────────────

    /// Returns `true` when the underlying WebSocket is open and the transport
    /// considers itself connected.
    pub fn is_connected(&self) -> bool {
        self.inner.borrow().is_connected()
    }

    /// Drop all pending in-flight requests and attempt to reconnect the WebSocket.
    /// Outstanding Promises for dropped requests will be rejected with `"ConnectionLost"`.
    pub fn reconnect(&mut self) -> bool {
        // Reject all pending promises before the internal queue is cleared.
        for (_, handle) in self.pending.borrow_mut().drain() {
            let _ = handle
                .reject
                .call1(&JsValue::NULL, &JsValue::from_str("ConnectionLost"));
        }
        self.inner.borrow_mut().reconnect().is_ok()
    }

    // ── Coil operations ──────────────────────────────────────────────────────

    /// Read `quantity` coils starting at `address`.
    ///
    /// Returns a `Promise` that resolves with a `Uint8Array` (bit-packed coil bytes)
    /// or rejects with an error string on failure.
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

    /// Write a single coil at `address` to `value` (true = ON, false = OFF).
    ///
    /// Returns a `Promise` that resolves with `{ address, value }` or rejects on error.
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

    // ── Register operations ───────────────────────────────────────────────────

    /// Read `quantity` holding registers starting at `address`.
    ///
    /// Returns a `Promise` that resolves with a `Uint16Array` (register values)
    /// or rejects with an error string on failure.
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
    /// Returns a `Promise` that resolves with a `Uint16Array` or rejects on error.
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
    /// Returns a `Promise` that resolves with `{ address, value }` or rejects on error.
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

    /// Write `values` to multiple consecutive holding registers starting at `address`.
    ///
    /// Returns a `Promise` that resolves with `{ address, quantity }` or rejects on error.
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

    // ── Discrete input operations ─────────────────────────────────────────────

    /// Read `quantity` discrete inputs starting at `address`.
    ///
    /// Returns a `Promise` that resolves with a `Uint8Array` (bit-packed)
    /// or rejects on error.
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

// ── Private helpers ───────────────────────────────────────────────────────────

impl WasmModbusClient {
    fn alloc_txn(&mut self) -> u16 {
        let id = self.next_txn;
        // Wrap around at u16::MAX, skipping 0 which some devices treat as broadcast.
        self.next_txn = self.next_txn.wrapping_add(1).max(1);
        id
    }

    /// Remove the pending entry and reject it synchronously.
    /// Used when request queuing fails before the frame is even sent.
    fn reject_immediate(&self, txn_id: u16, error: MbusError) {
        if let Some(handle) = self.pending.borrow_mut().remove(&txn_id) {
            let _ = handle
                .reject
                .call1(&JsValue::NULL, &JsValue::from_str(&format!("{:?}", error)));
        }
    }
}

// ── Promise constructor helper ────────────────────────────────────────────────

/// Creates a JS Promise and synchronously extracts the (resolve, reject) function pair.
///
/// The Promise executor runs synchronously, so the functions are guaranteed to be
/// populated by the time `Promise::new` returns.
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

#[wasm_bindgen]
impl WasmModbusClient {
    // ── Additional coil operations ────────────────────────────────────────────

    /// Read a single coil at `address`.
    ///
    /// Returns a `Promise` that resolves with a `boolean` or rejects on error.
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
    /// Returns a `Promise` that resolves with `{ address, quantity }` or rejects on error.
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

    // ── Additional register operations ────────────────────────────────────────

    /// Read a single holding register at `address`.
    ///
    /// Returns a `Promise` that resolves with a `number` or rejects on error.
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
    /// Returns a `Promise` that resolves with a `number` or rejects on error.
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

    // ── Additional discrete-input operations ──────────────────────────────────

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

    // ── FIFO queue operations ─────────────────────────────────────────────────

    /// Read the FIFO queue pointed to by `address` (FC 24).
    ///
    /// Returns a `Promise` resolving with a `Uint16Array` or rejects on error.
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

    // ── File record operations ────────────────────────────────────────────────

    /// Read a file record (FC 20).
    ///
    /// Returns a `Promise` resolving with `Array<{ fileNumber, recordNumber, data: Uint16Array }>`
    /// or rejects on error.
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

        // Copy JS slice into a heapless::Vec for the SubRequest builder.
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

    // ── Diagnostics operations ────────────────────────────────────────────────

    /// Read the exception status (FC 7) — serial-line only on most devices.
    ///
    /// Returns a `Promise` resolving with a status `number` or rejects on error.
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
    /// `object_id`: 0x00=VendorName, 0x01=ProductCode, 0x02=Revision, 0x03=VendorURL, etc.
    /// Returns a `Promise` resolving with `{ readDeviceIdCode, conformityLevel, moreFollows, objects }`
    /// or rejects on error.
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
