//! `WasmAppRouter` — implements the `mbus-client` application-layer traits.
//!
//! Instead of forwarding data to user-defined callback functions, every successful
//! response or error resolves/rejects a JS `Promise` that was stored in `pending`
//! when the request was originally queued.
//!
//! The `pending` map is an `Rc<RefCell<HashMap<u16, PendingHandle>>>` shared between
//! `WasmAppRouter` and `WasmModbusClient`. Both live on the same JS thread so
//! `Rc<RefCell>` is both safe and allocation-free.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use js_sys::{Array, Function, Object, Reflect, Uint8Array, Uint16Array};
use mbus_client::app::{
    CoilResponse, DiagnosticsResponse, DiscreteInputResponse, FifoQueueResponse,
    FileRecordResponse, RegisterResponse, RequestErrorNotifier,
};
use mbus_client::services::coil::Coils;
use mbus_client::services::diagnostic::DeviceIdentificationResponse;
use mbus_client::services::discrete_input::DiscreteInputs;
use mbus_client::services::fifo_queue::FifoQueue;
use mbus_client::services::file_record::SubRequestParams;
use mbus_client::services::register::Registers;
use mbus_core::errors::MbusError;
use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
use mbus_core::transport::{TimeKeeper, UnitIdOrSlaveAddr};
use wasm_bindgen::JsValue;

// ── Pending-request bookkeeping ───────────────────────────────────────────────

/// The JS Promise resolve/reject pair stored per in-flight transaction.
pub(super) struct PendingHandle {
    pub resolve: Function,
    pub reject: Function,
}

pub(super) type PendingMap = Rc<RefCell<HashMap<u16, PendingHandle>>>;

// ── WasmAppRouter ─────────────────────────────────────────────────────────────

pub(super) struct WasmAppRouter {
    pub(super) pending: PendingMap,
}

impl WasmAppRouter {
    pub(super) fn new(pending: PendingMap) -> Self {
        Self { pending }
    }

    /// Resolve a pending promise with a JS value; noop if txn_id is unknown.
    fn resolve(&self, txn_id: u16, value: JsValue) {
        if let Some(handle) = self.pending.borrow_mut().remove(&txn_id) {
            let _ = handle.resolve.call1(&JsValue::NULL, &value);
        }
    }

    /// Reject a pending promise with an error string; noop if txn_id is unknown.
    fn reject(&self, txn_id: u16, msg: &str) {
        if let Some(handle) = self.pending.borrow_mut().remove(&txn_id) {
            let _ = handle.reject.call1(&JsValue::NULL, &JsValue::from_str(msg));
        }
    }
}

// ── TimeKeeper ────────────────────────────────────────────────────────────────

impl TimeKeeper for WasmAppRouter {
    fn current_millis(&self) -> u64 {
        js_sys::Date::now() as u64
    }
}

// ── RequestErrorNotifier ──────────────────────────────────────────────────────

impl RequestErrorNotifier for WasmAppRouter {
    fn request_failed(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
    ) {
        self.reject(txn_id, &format!("{:?}", error));
    }
}

// ── CoilResponse ─────────────────────────────────────────────────────────────

impl CoilResponse for WasmAppRouter {
    fn read_coils_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        coils: &Coils,
    ) {
        // Only send the quantity-relevant bytes; values() returns a fixed [u8; MAX_COIL_BYTES].
        let needed = ((coils.quantity() + 7) / 8) as usize;
        let arr = Uint8Array::from(&coils.values()[..needed]);
        self.resolve(txn_id, arr.into());
    }

    fn read_single_coil_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: bool,
    ) {
        self.resolve(txn_id, JsValue::from_bool(value));
    }

    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("address"),
            &JsValue::from_f64(address as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("value"),
            &JsValue::from_bool(value),
        );
        self.resolve(txn_id, obj.into());
    }

    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("address"),
            &JsValue::from_f64(address as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("quantity"),
            &JsValue::from_f64(quantity as f64),
        );
        self.resolve(txn_id, obj.into());
    }
}

// ── RegisterResponse ─────────────────────────────────────────────────────────

impl RegisterResponse for WasmAppRouter {
    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        let arr = Uint16Array::from(&registers.values()[..registers.quantity() as usize]);
        self.resolve(txn_id, arr.into());
    }

    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: u16,
    ) {
        self.resolve(txn_id, JsValue::from_f64(value as f64));
    }

    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        let arr = Uint16Array::from(&registers.values()[..registers.quantity() as usize]);
        self.resolve(txn_id, arr.into());
    }

    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: u16,
    ) {
        self.resolve(txn_id, JsValue::from_f64(value as f64));
    }

    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("address"),
            &JsValue::from_f64(address as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("value"),
            &JsValue::from_f64(value as f64),
        );
        self.resolve(txn_id, obj.into());
    }

    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("address"),
            &JsValue::from_f64(starting_address as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("quantity"),
            &JsValue::from_f64(quantity as f64),
        );
        self.resolve(txn_id, obj.into());
    }

    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    ) {
        let arr = Uint16Array::from(&registers.values()[..registers.quantity() as usize]);
        self.resolve(txn_id, arr.into());
    }

    fn read_single_register_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: u16,
    ) {
        self.resolve(txn_id, JsValue::from_f64(value as f64));
    }

    fn mask_write_register_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
    ) {
        self.resolve(txn_id, JsValue::TRUE);
    }
}

// ── DiscreteInputResponse ─────────────────────────────────────────────────────

impl DiscreteInputResponse for WasmAppRouter {
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        discrete_inputs: &DiscreteInputs,
    ) {
        let arr = Uint8Array::from(discrete_inputs.values());
        self.resolve(txn_id, arr.into());
    }

    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        _address: u16,
        value: bool,
    ) {
        self.resolve(txn_id, JsValue::from_bool(value));
    }
}

// ── FifoQueueResponse ─────────────────────────────────────────────────────────

#[cfg(feature = "fifo")]
impl FifoQueueResponse for WasmAppRouter {
    fn read_fifo_queue_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        fifo_queue: &FifoQueue,
    ) {
        let arr = Uint16Array::from(&fifo_queue.queue()[..fifo_queue.length()]);
        self.resolve(txn_id, arr.into());
    }
}

// ── FileRecordResponse ────────────────────────────────────────────────────────

#[cfg(feature = "file-record")]
impl FileRecordResponse for WasmAppRouter {
    /// Resolves with `Array<{ fileNumber, recordNumber, data: Uint16Array }>`.
    fn read_file_record_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[SubRequestParams],
    ) {
        let arr = Array::new();
        for sub in data {
            let obj = Object::new();
            let _ = Reflect::set(
                &obj,
                &JsValue::from_str("fileNumber"),
                &JsValue::from_f64(sub.file_number as f64),
            );
            let _ = Reflect::set(
                &obj,
                &JsValue::from_str("recordNumber"),
                &JsValue::from_f64(sub.record_number as f64),
            );
            let js_data: JsValue = if let Some(rd) = &sub.record_data {
                Uint16Array::from(rd.as_slice()).into()
            } else {
                Uint16Array::new_with_length(0).into()
            };
            let _ = Reflect::set(&obj, &JsValue::from_str("data"), &js_data);
            arr.push(&obj.into());
        }
        self.resolve(txn_id, arr.into());
    }

    /// Resolves with `true`.
    fn write_file_record_response(&mut self, txn_id: u16, _unit_id_slave_addr: UnitIdOrSlaveAddr) {
        self.resolve(txn_id, JsValue::TRUE);
    }
}

// ── DiagnosticsResponse ───────────────────────────────────────────────────────

#[cfg(feature = "diagnostics")]
impl DiagnosticsResponse for WasmAppRouter {
    /// Resolves with `{ readDeviceIdCode, conformityLevel, moreFollows, objects: Array<{ id, value }> }`.
    fn read_device_identification_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        response: &DeviceIdentificationResponse,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("readDeviceIdCode"),
            &JsValue::from_f64(response.read_device_id_code as u8 as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("conformityLevel"),
            &JsValue::from_f64(response.conformity_level as u8 as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("moreFollows"),
            &JsValue::from_bool(response.more_follows),
        );
        let objects_arr = Array::new();
        for item in response.objects() {
            if let Ok(o) = item {
                let entry = Object::new();
                let id_byte: u8 = o.object_id.into();
                let _ = Reflect::set(
                    &entry,
                    &JsValue::from_str("id"),
                    &JsValue::from_f64(id_byte as f64),
                );
                let value_str = core::str::from_utf8(&o.value)
                    .map(|s| s.to_owned())
                    .unwrap_or_else(|_| {
                        o.value
                            .iter()
                            .map(|b| format!("{:02X}", b))
                            .collect::<std::vec::Vec<_>>()
                            .join(" ")
                    });
                let _ = Reflect::set(
                    &entry,
                    &JsValue::from_str("value"),
                    &JsValue::from_str(&value_str),
                );
                objects_arr.push(&entry.into());
            }
        }
        let _ = Reflect::set(&obj, &JsValue::from_str("objects"), &objects_arr);
        self.resolve(txn_id, obj.into());
    }

    /// Resolves with `{ meiType, data: Uint8Array }`.
    fn encapsulated_interface_transport_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("meiType"),
            &JsValue::from_f64(u8::from(mei_type) as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("data"),
            &Uint8Array::from(data).into(),
        );
        self.resolve(txn_id, obj.into());
    }

    /// Resolves with the status byte as a `number`.
    fn read_exception_status_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u8,
    ) {
        self.resolve(txn_id, JsValue::from_f64(status as f64));
    }

    /// Resolves with `{ subFunction, data: Uint16Array }`.
    fn diagnostics_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("subFunction"),
            &JsValue::from_f64(u16::from(sub_function) as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("data"),
            &Uint16Array::from(data).into(),
        );
        self.resolve(txn_id, obj.into());
    }

    /// Resolves with `{ status, eventCount }`.
    fn get_comm_event_counter_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("status"),
            &JsValue::from_f64(status as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("eventCount"),
            &JsValue::from_f64(event_count as f64),
        );
        self.resolve(txn_id, obj.into());
    }

    /// Resolves with `{ status, eventCount, messageCount, events: Uint8Array }`.
    fn get_comm_event_log_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
        message_count: u16,
        events: &[u8],
    ) {
        let obj = Object::new();
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("status"),
            &JsValue::from_f64(status as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("eventCount"),
            &JsValue::from_f64(event_count as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("messageCount"),
            &JsValue::from_f64(message_count as f64),
        );
        let _ = Reflect::set(
            &obj,
            &JsValue::from_str("events"),
            &Uint8Array::from(events).into(),
        );
        self.resolve(txn_id, obj.into());
    }

    /// Resolves with a `Uint8Array` containing the raw server ID data.
    fn report_server_id_response(
        &mut self,
        txn_id: u16,
        _unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[u8],
    ) {
        self.resolve(txn_id, Uint8Array::from(data).into());
    }
}
