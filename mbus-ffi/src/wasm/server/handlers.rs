//! WASM bindings to dispatch incoming Modbus requests to JS ServerHandlers callbacks.

use std::future::Future;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

use mbus_async::server::app_handler::{AsyncAppHandler, ModbusRequest, ModbusResponse};
use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;
use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[derive(Clone, Debug)]
pub struct JsServerHandlers {
    handlers_obj: JsValue,
}

unsafe impl Send for JsServerHandlers {}
unsafe impl Sync for JsServerHandlers {}

impl JsServerHandlers {
    /// Creates a new `JsServerHandlers` wrapping a JS callback registry object.
    pub fn new(handlers_obj: JsValue) -> Self {
        Self { handlers_obj }
    }
}

// ── App Handler Implementation ────────────────────────────────────────────────

impl AsyncAppHandler for JsServerHandlers {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let handlers = self.handlers_obj.clone();
        UnsafeSendFuture::new(async move {
            match req {
                #[cfg(feature = "coils")]
                ModbusRequest::ReadCoils {
                    unit,
                    address,
                    count,
                    ..
                } => Self::handle_read_coils(&handlers, unit, address, count).await,
                #[cfg(feature = "coils")]
                ModbusRequest::WriteSingleCoil {
                    unit,
                    address,
                    value,
                    ..
                } => Self::handle_write_single_coil(&handlers, unit, address, value).await,
                #[cfg(feature = "coils")]
                ModbusRequest::WriteMultipleCoils {
                    unit,
                    address,
                    count,
                    data,
                    ..
                } => {
                    Self::handle_write_multiple_coils(&handlers, unit, address, count, &data).await
                }

                #[cfg(feature = "discrete-inputs")]
                ModbusRequest::ReadDiscreteInputs {
                    unit,
                    address,
                    count,
                    ..
                } => Self::handle_read_discrete_inputs(&handlers, unit, address, count).await,

                #[cfg(feature = "holding-registers")]
                ModbusRequest::ReadHoldingRegisters {
                    unit,
                    address,
                    count,
                    ..
                } => Self::handle_read_holding_registers(&handlers, unit, address, count).await,
                #[cfg(feature = "holding-registers")]
                ModbusRequest::WriteSingleRegister {
                    unit,
                    address,
                    value,
                    ..
                } => Self::handle_write_single_register(&handlers, unit, address, value).await,
                #[cfg(feature = "holding-registers")]
                ModbusRequest::WriteMultipleRegisters {
                    unit,
                    address,
                    count,
                    data,
                    ..
                } => {
                    Self::handle_write_multiple_registers(&handlers, unit, address, count, &data)
                        .await
                }
                #[cfg(feature = "holding-registers")]
                ModbusRequest::MaskWriteRegister {
                    unit,
                    address,
                    and_mask,
                    or_mask,
                    ..
                } => {
                    Self::handle_mask_write_register(&handlers, unit, address, and_mask, or_mask)
                        .await
                }
                #[cfg(feature = "holding-registers")]
                ModbusRequest::ReadWriteMultipleRegisters {
                    unit,
                    read_address,
                    read_count,
                    write_address,
                    write_count: _,
                    data,
                    ..
                } => {
                    Self::handle_read_write_multiple_registers(
                        &handlers,
                        unit,
                        read_address,
                        read_count,
                        write_address,
                        &data,
                    )
                    .await
                }

                #[cfg(feature = "input-registers")]
                ModbusRequest::ReadInputRegisters {
                    unit,
                    address,
                    count,
                    ..
                } => Self::handle_read_input_registers(&handlers, unit, address, count).await,

                #[cfg(feature = "fifo")]
                ModbusRequest::ReadFifoQueue {
                    unit,
                    pointer_address,
                    ..
                } => Self::handle_read_fifo_queue(&handlers, unit, pointer_address).await,

                #[cfg(feature = "file-record")]
                ModbusRequest::ReadFileRecord {
                    unit, sub_requests, ..
                } => Self::handle_read_file_record(&handlers, unit, &sub_requests).await,
                #[cfg(feature = "file-record")]
                ModbusRequest::WriteFileRecord {
                    unit,
                    sub_requests,
                    raw_pdu_data,
                    ..
                } => {
                    Self::handle_write_file_record(&handlers, unit, &sub_requests, raw_pdu_data)
                        .await
                }

                #[cfg(feature = "diagnostics")]
                ModbusRequest::ReadExceptionStatus { unit, .. } => {
                    Self::handle_read_exception_status(&handlers, unit).await
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::Diagnostics {
                    unit,
                    sub_function,
                    data,
                    ..
                } => Self::handle_diagnostics(&handlers, unit, sub_function, data).await,
                #[cfg(feature = "diagnostics")]
                ModbusRequest::EncapsulatedInterfaceTransport {
                    unit,
                    mei_type,
                    data,
                    ..
                } => {
                    Self::handle_encapsulated_interface_transport(&handlers, unit, mei_type, &data)
                        .await
                }

                ModbusRequest::Unknown { function_code, .. } => {
                    ModbusResponse::exception_raw(function_code, ExceptionCode::IllegalFunction)
                }
                _ => ModbusResponse::NoResponse,
            }
        })
    }
}

// ── Private Handlers ──────────────────────────────────────────────────────────

impl JsServerHandlers {
    #[cfg(feature = "coils")]
    async fn handle_read_coils(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadCoils;
        if let Some(func) = get_handler_fn(handlers, "onReadCoils") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("quantity"),
                &JsValue::from_f64(count as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(coils) = to_bool_vec(&val) {
                        if coils.len() == count as usize {
                            let packed = pack_coils(&coils);
                            ModbusResponse::packed_bits(fc, &packed)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "coils")]
    async fn handle_write_single_coil(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> ModbusResponse {
        let fc = FunctionCode::WriteSingleCoil;
        if let Some(func) = get_handler_fn(handlers, "onWriteSingleCoil") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("value"),
                &JsValue::from_bool(value),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_coil(address, value)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_coil(address, value)
        }
    }

    #[cfg(feature = "coils")]
    async fn handle_write_multiple_coils(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
        data: &[u8],
    ) -> ModbusResponse {
        let fc = FunctionCode::WriteMultipleCoils;
        if let Some(func) = get_handler_fn(handlers, "onWriteMultipleCoils") {
            // Unpack coils from bytes
            let mut values = std::vec::Vec::with_capacity(count as usize);
            for i in 0..count {
                let byte_idx = (i / 8) as usize;
                let bit_idx = i % 8;
                let val = if byte_idx < data.len() {
                    (data[byte_idx] & (1 << bit_idx)) != 0
                } else {
                    false
                };
                values.push(val);
            }

            let js_values = js_sys::Array::new();
            for &val in &values {
                js_values.push(&JsValue::from_bool(val));
            }

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(&req, &JsValue::from_str("values"), &js_values.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_multi_write(fc, address, count)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_multi_write(fc, address, count)
        }
    }

    #[cfg(feature = "discrete-inputs")]
    async fn handle_read_discrete_inputs(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadDiscreteInputs;
        if let Some(func) = get_handler_fn(handlers, "onReadDiscreteInputs") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("quantity"),
                &JsValue::from_f64(count as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(coils) = to_bool_vec(&val) {
                        if coils.len() == count as usize {
                            let packed = pack_coils(&coils);
                            ModbusResponse::packed_bits(fc, &packed)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "holding-registers")]
    async fn handle_read_holding_registers(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadHoldingRegisters;
        if let Some(func) = get_handler_fn(handlers, "onReadHoldingRegisters") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("quantity"),
                &JsValue::from_f64(count as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(regs) = to_u16_vec(&val) {
                        if regs.len() == count as usize {
                            ModbusResponse::registers(fc, &regs)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "holding-registers")]
    async fn handle_write_single_register(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::WriteSingleRegister;
        if let Some(func) = get_handler_fn(handlers, "onWriteSingleRegister") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("value"),
                &JsValue::from_f64(value as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_register(address, value)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_register(address, value)
        }
    }

    #[cfg(feature = "holding-registers")]
    async fn handle_write_multiple_registers(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
        data: &[u8],
    ) -> ModbusResponse {
        let fc = FunctionCode::WriteMultipleRegisters;
        if let Some(func) = get_handler_fn(handlers, "onWriteMultipleRegisters") {
            let mut values = std::vec::Vec::with_capacity(count as usize);
            for i in 0..count {
                let idx = (i * 2) as usize;
                if idx + 1 < data.len() {
                    values.push(u16::from_be_bytes([data[idx], data[idx + 1]]));
                }
            }

            let js_values = js_sys::Array::new();
            for &val in &values {
                js_values.push(&JsValue::from_f64(val as f64));
            }

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(&req, &JsValue::from_str("values"), &js_values.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_multi_write(fc, address, count)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_multi_write(fc, address, count)
        }
    }

    #[cfg(feature = "holding-registers")]
    async fn handle_mask_write_register(
        handlers: &JsValue,
        _unit: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::MaskWriteRegister;
        // Node.js ServerHandlers interface does not have onMaskWriteRegister directly,
        // it returns IllegalFunction by default (which matches original behavior).
        // Let's implement support if onMaskWriteRegister existed, otherwise echo.
        if let Some(func) = get_handler_fn(handlers, "onMaskWriteRegister") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("andMask"),
                &JsValue::from_f64(and_mask as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("orMask"),
                &JsValue::from_f64(or_mask as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_mask_write(address, and_mask, or_mask)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_mask_write(address, and_mask, or_mask)
        }
    }

    #[cfg(feature = "holding-registers")]
    async fn handle_read_write_multiple_registers(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        read_address: u16,
        read_count: u16,
        write_address: u16,
        data: &[u8],
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadWriteMultipleRegisters;
        if let Some(func) = get_handler_fn(handlers, "onReadWriteMultipleRegisters") {
            let count = (data.len() / 2) as u16;
            let mut write_values = std::vec::Vec::with_capacity(count as usize);
            for i in 0..count {
                let idx = (i * 2) as usize;
                if idx + 1 < data.len() {
                    write_values.push(u16::from_be_bytes([data[idx], data[idx + 1]]));
                }
            }

            let js_values = js_sys::Array::new();
            for &val in &write_values {
                js_values.push(&JsValue::from_f64(val as f64));
            }

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("readAddress"),
                &JsValue::from_f64(read_address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("readQuantity"),
                &JsValue::from_f64(read_count as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("writeAddress"),
                &JsValue::from_f64(write_address as f64),
            );
            let _ =
                js_sys::Reflect::set(&req, &JsValue::from_str("writeValues"), &js_values.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(regs) = to_u16_vec(&val) {
                        if regs.len() == read_count as usize {
                            ModbusResponse::registers(fc, &regs)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "input-registers")]
    async fn handle_read_input_registers(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
        count: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadInputRegisters;
        if let Some(func) = get_handler_fn(handlers, "onReadInputRegisters") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("quantity"),
                &JsValue::from_f64(count as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(regs) = to_u16_vec(&val) {
                        if regs.len() == count as usize {
                            ModbusResponse::registers(fc, &regs)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "fifo")]
    async fn handle_read_fifo_queue(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        address: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadFifoQueue;
        if let Some(func) = get_handler_fn(handlers, "onReadFifoQueue") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("address"),
                &JsValue::from_f64(address as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(values) = to_u16_vec(&val) {
                        if values.len() <= 31 {
                            let count = values.len() as u16;
                            let mut payload = std::vec::Vec::new();
                            payload.extend_from_slice(&count.to_be_bytes());
                            for v in values {
                                payload.extend_from_slice(&v.to_be_bytes());
                            }
                            ModbusResponse::fifo_response(&payload)
                        } else {
                            ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress)
                        }
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "file-record")]
    async fn handle_read_file_record(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        sub_requests: &[mbus_core::models::file_record::FileRecordReadSubRequest],
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadFileRecord;
        if let Some(func) = get_handler_fn(handlers, "onReadFileRecord") {
            let js_reqs = js_sys::Array::new();
            for sub in sub_requests {
                let item = js_sys::Object::new();
                let _ = js_sys::Reflect::set(
                    &item,
                    &JsValue::from_str("fileNumber"),
                    &JsValue::from_f64(sub.file_number as f64),
                );
                let _ = js_sys::Reflect::set(
                    &item,
                    &JsValue::from_str("recordNumber"),
                    &JsValue::from_f64(sub.record_number as f64),
                );
                let _ = js_sys::Reflect::set(
                    &item,
                    &JsValue::from_str("recordLength"),
                    &JsValue::from_f64(sub.record_length as f64),
                );
                js_reqs.push(&item.into());
            }

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(&req, &JsValue::from_str("requests"), &js_reqs.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Ok(nested) = to_nested_u16_vec(&val) {
                        let payload = build_file_record_payload(&nested);
                        ModbusResponse::read_file_record_response(&payload)
                    } else {
                        ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "file-record")]
    async fn handle_write_file_record(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        sub_requests: &[mbus_async::server::app_handler::AsyncFileRecordWriteSubRequest],
        raw_pdu_data: heapless::Vec<u8, MAX_PDU_DATA_LEN>,
    ) -> ModbusResponse {
        let fc = FunctionCode::WriteFileRecord;
        if let Some(func) = get_handler_fn(handlers, "onWriteFileRecord") {
            let js_reqs = js_sys::Array::new();
            for sub in sub_requests {
                let item = js_sys::Object::new();
                let _ = js_sys::Reflect::set(
                    &item,
                    &JsValue::from_str("fileNumber"),
                    &JsValue::from_f64(sub.file_number as f64),
                );
                let _ = js_sys::Reflect::set(
                    &item,
                    &JsValue::from_str("recordNumber"),
                    &JsValue::from_f64(sub.record_number as f64),
                );

                let data_arr = js_sys::Array::new();
                for i in 0..sub.record_length {
                    let idx = (i * 2) as usize;
                    if idx + 1 < sub.record_data.len() {
                        let val =
                            u16::from_be_bytes([sub.record_data[idx], sub.record_data[idx + 1]]);
                        data_arr.push(&JsValue::from_f64(val as f64));
                    }
                }
                let _ =
                    js_sys::Reflect::set(&item, &JsValue::from_str("recordData"), &data_arr.into());
                js_reqs.push(&item.into());
            }

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(&req, &JsValue::from_str("requests"), &js_reqs.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else {
                        ModbusResponse::echo_write_file_record(raw_pdu_data)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::echo_write_file_record(raw_pdu_data)
        }
    }

    #[cfg(feature = "diagnostics")]
    async fn handle_read_exception_status(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
    ) -> ModbusResponse {
        let fc = FunctionCode::ReadExceptionStatus;
        if let Some(func) = get_handler_fn(handlers, "onReadExceptionStatus") {
            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if let Some(status) = val.as_f64() {
                        ModbusResponse::read_exception_status(status as u8)
                    } else {
                        ModbusResponse::read_exception_status(0)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::read_exception_status(0)
        }
    }

    #[cfg(feature = "diagnostics")]
    async fn handle_diagnostics(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        sub_function: u16,
        data: u16,
    ) -> ModbusResponse {
        let fc = FunctionCode::Diagnostics;
        if let Some(func) = get_handler_fn(handlers, "onDiagnostics") {
            let js_data = js_sys::Array::new();
            js_data.push(&JsValue::from_f64(data as f64));

            let req = js_sys::Object::new();
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("unitId"),
                &JsValue::from_f64(u8::from(unit) as f64),
            );
            let _ = js_sys::Reflect::set(
                &req,
                &JsValue::from_str("subFunction"),
                &JsValue::from_f64(sub_function as f64),
            );
            let _ = js_sys::Reflect::set(&req, &JsValue::from_str("data"), &js_data.into());

            match call_handler(&func, &req.into()).await {
                Ok(val) => {
                    if let Some(exc) = get_exception_code(&val) {
                        ModbusResponse::exception(fc, exc)
                    } else if val.is_object() {
                        let resp_sub_fn = if let Ok(sub) =
                            js_sys::Reflect::get(&val, &JsValue::from_str("subFunction"))
                        {
                            sub.as_f64().unwrap_or(sub_function as f64) as u16
                        } else {
                            sub_function
                        };
                        let resp_data = if let Ok(d_val) =
                            js_sys::Reflect::get(&val, &JsValue::from_str("data"))
                        {
                            if let Ok(d_vec) = to_u16_vec(&d_val) {
                                d_vec.first().copied().unwrap_or(data)
                            } else {
                                data
                            }
                        } else {
                            data
                        };
                        ModbusResponse::diagnostics_echo(resp_sub_fn, resp_data)
                    } else {
                        ModbusResponse::diagnostics_echo(sub_function, data)
                    }
                }
                Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
            }
        } else {
            ModbusResponse::diagnostics_echo(sub_function, data)
        }
    }

    #[cfg(feature = "diagnostics")]
    async fn handle_encapsulated_interface_transport(
        handlers: &JsValue,
        unit: UnitIdOrSlaveAddr,
        mei_type: u8,
        data: &[u8],
    ) -> ModbusResponse {
        let fc = FunctionCode::EncapsulatedInterfaceTransport;
        if mei_type == 0x0E {
            if data.len() < 2 {
                return ModbusResponse::exception(fc, ExceptionCode::IllegalDataValue);
            }
            let read_device_id_code = data[0];
            let start_object_id = data[1];

            if let Some(func) = get_handler_fn(handlers, "onReadDeviceIdentification") {
                let req = js_sys::Object::new();
                let _ = js_sys::Reflect::set(
                    &req,
                    &JsValue::from_str("unitId"),
                    &JsValue::from_f64(u8::from(unit) as f64),
                );
                let _ = js_sys::Reflect::set(
                    &req,
                    &JsValue::from_str("readDeviceIdCode"),
                    &JsValue::from_f64(read_device_id_code as f64),
                );
                let _ = js_sys::Reflect::set(
                    &req,
                    &JsValue::from_str("objectId"),
                    &JsValue::from_f64(start_object_id as f64),
                );

                match call_handler(&func, &req.into()).await {
                    Ok(val) => {
                        if let Some(exc) = get_exception_code(&val) {
                            ModbusResponse::exception(fc, exc)
                        } else if val.is_object() {
                            let (conformity_level, more_follows, next_object_id, objects_bytes) =
                                Self::parse_device_identification_response(&val);
                            ModbusResponse::read_device_id(
                                read_device_id_code,
                                conformity_level,
                                more_follows,
                                next_object_id,
                                &objects_bytes,
                            )
                        } else {
                            Self::default_device_id_response(read_device_id_code, start_object_id)
                        }
                    }
                    Err(_) => ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure),
                }
            } else {
                Self::default_device_id_response(read_device_id_code, start_object_id)
            }
        } else {
            ModbusResponse::exception(fc, ExceptionCode::IllegalFunction)
        }
    }

    #[cfg(feature = "diagnostics")]
    fn parse_device_identification_response(val: &JsValue) -> (u8, bool, u8, Vec<u8>) {
        let conformity_level = js_sys::Reflect::get(val, &JsValue::from_str("conformityLevel"))
            .ok()
            .and_then(|c| c.as_f64())
            .map(|c| c as u8)
            .unwrap_or(0x82);

        let more_follows = js_sys::Reflect::get(val, &JsValue::from_str("moreFollows"))
            .ok()
            .and_then(|m| m.as_bool())
            .unwrap_or(false);

        let next_object_id = js_sys::Reflect::get(val, &JsValue::from_str("nextObjectId"))
            .ok()
            .and_then(|n| n.as_f64())
            .map(|n| n as u8)
            .unwrap_or(0);

        let objects_bytes = js_sys::Reflect::get(val, &JsValue::from_str("objects"))
            .ok()
            .filter(|objs_val| js_sys::Array::is_array(objs_val))
            .map(|objs_val| Self::parse_device_identification_objects(&objs_val))
            .unwrap_or_default();

        (
            conformity_level,
            more_follows,
            next_object_id,
            objects_bytes,
        )
    }

    #[cfg(feature = "diagnostics")]
    fn parse_device_identification_objects(objs_val: &JsValue) -> Vec<u8> {
        let mut objects_bytes = std::vec::Vec::new();
        let arr = js_sys::Array::from(objs_val);
        for i in 0..arr.length() {
            let item = arr.get(i);
            if item.is_object() {
                let obj_id = js_sys::Reflect::get(&item, &JsValue::from_str("id"))
                    .ok()
                    .and_then(|id_val| id_val.as_f64())
                    .map(|id| id as u8)
                    .unwrap_or(0);

                let obj_val_str = js_sys::Reflect::get(&item, &JsValue::from_str("value"))
                    .ok()
                    .and_then(|val_val| val_val.as_string())
                    .unwrap_or_default();

                let val_bytes = obj_val_str.as_bytes();
                objects_bytes.push(obj_id);
                objects_bytes.push(val_bytes.len() as u8);
                objects_bytes.extend_from_slice(val_bytes);
            }
        }
        objects_bytes
    }

    #[cfg(feature = "diagnostics")]
    fn default_device_id_response(read_device_id_code: u8, start_object_id: u8) -> ModbusResponse {
        let vendor_name = b"Modbus-RS WASM Server";
        let product_code = b"mbus-wasm-server";
        let revision = b"0.15.0";
        let vendor_url = b"https://github.com/Raghava-Ch/modbus-rs";
        let product_name = b"WASM Server Simulator";

        let objects: &[(u8, &[u8])] = &[
            (0x00, vendor_name),
            (0x01, product_code),
            (0x02, revision),
            (0x03, vendor_url),
            (0x04, product_name),
        ];

        let mut objects_bytes = std::vec::Vec::new();
        let conformity = 0x82;

        if read_device_id_code == 0x04 {
            for &(id, val) in objects {
                if id == start_object_id {
                    objects_bytes.push(id);
                    objects_bytes.push(val.len() as u8);
                    objects_bytes.extend_from_slice(val);
                    return ModbusResponse::read_device_id(
                        read_device_id_code,
                        conformity,
                        false,
                        0,
                        &objects_bytes,
                    );
                }
            }
            return ModbusResponse::exception(
                FunctionCode::EncapsulatedInterfaceTransport,
                ExceptionCode::IllegalDataAddress,
            );
        }

        let more_follows = false;
        let next_id = 0;

        for &(id, val) in objects.iter().filter(|&&(id, _)| id >= start_object_id) {
            objects_bytes.push(id);
            objects_bytes.push(val.len() as u8);
            objects_bytes.extend_from_slice(val);
        }

        ModbusResponse::read_device_id(
            read_device_id_code,
            conformity,
            more_follows,
            next_id,
            &objects_bytes,
        )
    }
}

// ── Shared Helpers ────────────────────────────────────────────────────────────

fn get_handler_fn(obj: &JsValue, name: &str) -> Option<js_sys::Function> {
    if obj.is_object() {
        if let Ok(val) = js_sys::Reflect::get(obj, &JsValue::from_str(name)) {
            if val.is_function() {
                return val.dyn_into::<js_sys::Function>().ok();
            }
        }
    }
    None
}

async fn call_handler(func: &js_sys::Function, req_obj: &JsValue) -> Result<JsValue, JsValue> {
    let result = func.call1(&JsValue::NULL, req_obj)?;
    if let Ok(promise) = result.clone().dyn_into::<js_sys::Promise>() {
        let res = wasm_bindgen_futures::JsFuture::from(promise).await;
        res
    } else {
        Ok(result)
    }
}

fn get_exception_code(val: &JsValue) -> Option<ExceptionCode> {
    if val.is_object() {
        if let Ok(arr) = val.clone().dyn_into::<js_sys::Array>() {
            // Arrays are objects in js, but not Exception objects
            let _ = arr;
            return None;
        }
        if let Ok(code_val) = js_sys::Reflect::get(val, &JsValue::from_str("exception")) {
            if let Some(code) = code_val.as_f64() {
                match code as u8 {
                    0x01 => return Some(ExceptionCode::IllegalFunction),
                    0x02 => return Some(ExceptionCode::IllegalDataAddress),
                    0x03 => return Some(ExceptionCode::IllegalDataValue),
                    0x04 => return Some(ExceptionCode::ServerDeviceFailure),
                    0x0A => return Some(ExceptionCode::GatewayPathUnavailable),
                    0x0B => return Some(ExceptionCode::GatewayTargetDeviceFailedToRespond),
                    _ => {}
                }
            }
        }
    }
    None
}

fn to_bool_vec(val: &JsValue) -> Result<std::vec::Vec<bool>, String> {
    if js_sys::Array::is_array(val) {
        let arr = js_sys::Array::from(val);
        let mut vec = std::vec::Vec::with_capacity(arr.length() as usize);
        for i in 0..arr.length() {
            let item = arr.get(i);
            vec.push(item.as_bool().unwrap_or(false));
        }
        Ok(vec)
    } else {
        Err("Expected boolean[]".to_string())
    }
}

fn to_u16_vec(val: &JsValue) -> Result<std::vec::Vec<u16>, String> {
    if js_sys::Array::is_array(val) {
        let arr = js_sys::Array::from(val);
        let mut vec = std::vec::Vec::with_capacity(arr.length() as usize);
        for i in 0..arr.length() {
            let item = arr.get(i);
            if let Some(num) = item.as_f64() {
                vec.push(num as u16);
            } else {
                return Err("Array element is not a number".to_string());
            }
        }
        Ok(vec)
    } else if let Ok(typed_arr) = val.clone().dyn_into::<js_sys::Uint16Array>() {
        Ok(typed_arr.to_vec())
    } else {
        Err("Expected number[] or Uint16Array".to_string())
    }
}

#[cfg(feature = "file-record")]
fn to_nested_u16_vec(val: &JsValue) -> Result<std::vec::Vec<std::vec::Vec<u16>>, String> {
    if js_sys::Array::is_array(val) {
        let arr = js_sys::Array::from(val);
        let mut vec = std::vec::Vec::with_capacity(arr.length() as usize);
        for i in 0..arr.length() {
            let item = arr.get(i);
            let inner = to_u16_vec(&item)?;
            vec.push(inner);
        }
        Ok(vec)
    } else {
        Err("Expected nested array".to_string())
    }
}

fn pack_coils(coils: &[bool]) -> std::vec::Vec<u8> {
    let mut bytes = std::vec::Vec::new();
    for (i, &bit) in coils.iter().enumerate() {
        let byte_idx = i / 8;
        let bit_idx = i % 8;
        if byte_idx >= bytes.len() {
            bytes.push(0);
        }
        if bit {
            bytes[byte_idx] |= 1 << bit_idx;
        }
    }
    bytes
}

#[cfg(feature = "file-record")]
fn build_file_record_payload(records: &[std::vec::Vec<u16>]) -> std::vec::Vec<u8> {
    let mut payload = std::vec::Vec::new();
    for record in records {
        let record_bytes_len = record.len() * 2;
        let file_record_len = (1 + record_bytes_len) as u8;
        payload.push(file_record_len);
        payload.push(0x06); // reference type
        for &val in record {
            payload.extend_from_slice(&val.to_be_bytes());
        }
    }
    payload
}

#[cfg(feature = "traffic")]
impl mbus_async::server::AsyncServerTrafficNotifier for JsServerHandlers {}

struct UnsafeSendFuture<F> {
    inner: F,
}

impl<F> UnsafeSendFuture<F> {
    fn new(inner: F) -> Self {
        Self { inner }
    }
}

// Safety: This is compiled for the WASM target which is single-threaded.
unsafe impl<F> Send for UnsafeSendFuture<F> {}

impl<F: std::future::Future> std::future::Future for UnsafeSendFuture<F> {
    type Output = F::Output;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let unsafe_self = unsafe { self.get_unchecked_mut() };
        let inner_pin = unsafe { std::pin::Pin::new_unchecked(&mut unsafe_self.inner) };
        inner_pin.poll(cx)
    }
}
