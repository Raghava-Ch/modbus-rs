//! Node.js bindings for the async Modbus TCP server.

use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunction;
use napi_derive::napi;
use serde_json::Value as JsValue;

use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server_async::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::nodejs::errors::{ERR_MODBUS_INVALID_ARGUMENT, to_napi_err};
use crate::nodejs::runtime;

// ── Option structs ───────────────────────────────────────────────────────────

/// Server bind options.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct TcpServerOptions {
    /// Bind host address (e.g., "0.0.0.0").
    pub host: String,
    /// Bind port.
    pub port: u16,
    /// Modbus unit ID to respond to.
    pub unit_id: u8,
}

/// Handler request for reading coils.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadCoilsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for writing a single coil.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteSingleCoilRequest {
    pub unit_id: u8,
    pub address: u16,
    pub value: bool,
}

/// Handler request for writing multiple coils.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteMultipleCoilsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub values: Vec<bool>,
}

/// Handler request for reading discrete inputs.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadDiscreteInputsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for reading holding registers.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadHoldingRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for reading input registers.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadInputRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for writing a single register.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteSingleRegisterRequest {
    pub unit_id: u8,
    pub address: u16,
    pub value: u16,
}

/// Handler request for writing multiple registers.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct WriteMultipleRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub values: Vec<u16>,
}

/// Handler request for reading FIFO queue.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadFifoQueueRequest {
    pub unit_id: u8,
    pub address: u16,
}

/// Handler request for diagnostics.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiagnosticsRequest {
    pub unit_id: u8,
    pub sub_function: u16,
    pub data: Vec<u16>,
}

/// Response that may include an exception code.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerExceptionResponse {
    /// Modbus exception code if this is an error response.
    pub exception: Option<u8>,
}

// ── Diagnostics Response ─────────────────────────────────────────────────────

/// Handler response for diagnostics.
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerDiagnosticsResponse {
    pub sub_function: u16,
    pub data: Vec<u16>,
}

// ── JS return value inspection ────────────────────────────────────────────────

/// Describes how we interpret the JS callback's return value.
enum HandlerReturn<T> {
    /// JS returned `{ exception: N }` — send a Modbus exception.
    Exception(ExceptionCode),
    /// JS returned usable data — success path.
    Data(T),
    /// JS returned `null` / `undefined` — treat as "no data" (write-echo path).
    Void,
}

fn u8_to_exception_code(val: u8) -> Option<ExceptionCode> {
    match val {
        0x01 => Some(ExceptionCode::IllegalFunction),
        0x02 => Some(ExceptionCode::IllegalDataAddress),
        0x03 => Some(ExceptionCode::IllegalDataValue),
        0x04 => Some(ExceptionCode::ServerDeviceFailure),
        0x0A => Some(ExceptionCode::GatewayPathUnavailable),
        0x0B => Some(ExceptionCode::GatewayTargetDeviceFailedToRespond),
        _ => None,
    }
}

/// Check if a `serde_json::Value` is an exception object `{ exception: N }`.
///
/// Only matches plain objects (not arrays) with a numeric `exception` key.
fn try_exception_code(val: &JsValue) -> Option<ExceptionCode> {
    // Arrays also report as Object in JS; skip them
    if val.is_array() {
        return None;
    }
    let obj = val.as_object()?;
    let exc = obj.get("exception")?;
    let code = exc.as_u64()? as u8;
    u8_to_exception_code(code)
}

/// Inspect a JS write-handler return value (expected: undefined/null or exception object).
fn parse_write_return(val: Option<JsValue>) -> HandlerReturn<()> {
    match val {
        None => HandlerReturn::Void,
        Some(v) => {
            if let Some(exc) = try_exception_code(&v) {
                HandlerReturn::Exception(exc)
            } else {
                HandlerReturn::Void
            }
        }
    }
}

/// Inspect a JS read-handler return value that should be `Vec<u16>` or exception.
fn parse_u16_vec_return(val: Option<JsValue>) -> HandlerReturn<Vec<u16>> {
    let val = match val {
        None => return HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure),
        Some(v) => v,
    };
    if let Some(exc) = try_exception_code(&val) {
        return HandlerReturn::Exception(exc);
    }
    if let JsValue::Array(arr) = &val {
        let mut result = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(n) = item.as_u64() {
                result.push(n as u16);
            } else {
                return HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure);
            }
        }
        return HandlerReturn::Data(result);
    }
    HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure)
}

/// Inspect a JS read-handler return value that should be `Vec<bool>` or exception.
fn parse_bool_vec_return(val: Option<JsValue>) -> HandlerReturn<Vec<bool>> {
    let val = match val {
        None => return HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure),
        Some(v) => v,
    };
    if let Some(exc) = try_exception_code(&val) {
        return HandlerReturn::Exception(exc);
    }
    if let JsValue::Array(arr) = &val {
        let mut result = Vec::with_capacity(arr.len());
        for item in arr {
            if let Some(b) = item.as_bool() {
                result.push(b);
            } else {
                return HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure);
            }
        }
        return HandlerReturn::Data(result);
    }
    HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure)
}

/// Inspect a JS diagnostics-handler return value.
fn parse_diagnostics_return(val: Option<JsValue>) -> HandlerReturn<ServerDiagnosticsResponse> {
    let val = match val {
        None => return HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure),
        Some(v) => v,
    };
    if let Some(exc) = try_exception_code(&val) {
        return HandlerReturn::Exception(exc);
    }
    if let Ok(resp) = serde_json::from_value::<ServerDiagnosticsResponse>(val) {
        return HandlerReturn::Data(resp);
    }
    HandlerReturn::Exception(ExceptionCode::ServerDeviceFailure)
}

// ── Handler type alias ────────────────────────────────────────────────────────

/// The JS handler returns a Promise that resolves to an optional JSON value.
///
/// - `None` (undefined/null resolve) → write-echo or void success
/// - `Some(Value::Array(...))` → register/coil data
/// - `Some(Value::Object({exception: N}))` → Modbus exception
type JsHandler<Req> = ThreadsafeFunction<Req, Promise<Option<JsValue>>, Req, napi::Status, false>;

// ── Handler struct ───────────────────────────────────────────────────────────

/// Internal adapter that implements AsyncAppHandler by delegating to JS callbacks.
///
/// All handler fields use `Promise<Option<serde_json::Value>>` as the
/// ThreadsafeFunction return type. The JS handler returns a Promise (since it's
/// async). We receive it, await it, then inspect the resolved value to determine
/// if it's an exception object, data array, or void (undefined/null).
#[derive(Clone)]
pub struct JsHandlerAdapter {
    #[cfg(feature = "coils")]
    pub on_read_coils: Option<Arc<JsHandler<ReadCoilsRequest>>>,
    #[cfg(feature = "coils")]
    pub on_write_single_coil: Option<Arc<JsHandler<WriteSingleCoilRequest>>>,
    #[cfg(feature = "coils")]
    pub on_write_multiple_coils: Option<Arc<JsHandler<WriteMultipleCoilsRequest>>>,
    #[cfg(feature = "discrete-inputs")]
    pub on_read_discrete_inputs: Option<Arc<JsHandler<ReadDiscreteInputsRequest>>>,
    #[cfg(feature = "holding-registers")]
    pub on_read_holding_registers: Option<Arc<JsHandler<ReadHoldingRegistersRequest>>>,
    #[cfg(feature = "input-registers")]
    pub on_read_input_registers: Option<Arc<JsHandler<ReadInputRegistersRequest>>>,
    #[cfg(feature = "holding-registers")]
    pub on_write_single_register: Option<Arc<JsHandler<WriteSingleRegisterRequest>>>,
    #[cfg(feature = "holding-registers")]
    pub on_write_multiple_registers: Option<Arc<JsHandler<WriteMultipleRegistersRequest>>>,
    #[cfg(feature = "fifo")]
    pub on_read_fifo_queue: Option<Arc<JsHandler<ReadFifoQueueRequest>>>,
    #[cfg(feature = "diagnostics")]
    pub on_diagnostics: Option<Arc<JsHandler<DiagnosticsRequest>>>,
}

// Safety: The ThreadsafeFunctions are designed to be called from multiple threads
unsafe impl Send for JsHandlerAdapter {}
unsafe impl Sync for JsHandlerAdapter {}

#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncServerTrafficNotifier for JsHandlerAdapter {}

fn pack_bits(
    bits: &[bool],
) -> heapless::Vec<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }> {
    let mut packed = heapless::Vec::new();
    for chunk in bits.chunks(8) {
        let mut byte: u8 = 0;
        for (i, &b) in chunk.iter().enumerate() {
            if b {
                byte |= 1 << i;
            }
        }
        let _ = packed.push(byte);
    }
    packed
}

impl AsyncAppHandler for JsHandlerAdapter {
    async fn handle(&mut self, req: ModbusRequest) -> ModbusResponse {
        match req {
            #[cfg(feature = "coils")]
            ModbusRequest::ReadCoils {
                address,
                count,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_read_coils {
                    let js_req = ReadCoilsRequest {
                        unit_id: u8::from(unit),
                        address,
                        quantity: count,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_bool_vec_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::ReadCoils, e)
                                }
                                HandlerReturn::Data(bits) => {
                                    if bits.len() == count as usize {
                                        ModbusResponse::packed_bits(
                                            FunctionCode::ReadCoils,
                                            &pack_bits(&bits),
                                        )
                                    } else {
                                        ModbusResponse::exception(
                                            FunctionCode::ReadCoils,
                                            ExceptionCode::IllegalDataAddress,
                                        )
                                    }
                                }
                                HandlerReturn::Void => ModbusResponse::exception(
                                    FunctionCode::ReadCoils,
                                    ExceptionCode::ServerDeviceFailure,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::ReadCoils,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::ReadCoils,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadCoils,
                        ExceptionCode::IllegalFunction,
                    )
                }
            }
            #[cfg(feature = "coils")]
            ModbusRequest::WriteSingleCoil {
                address,
                value,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_write_single_coil {
                    let js_req = WriteSingleCoilRequest {
                        unit_id: u8::from(unit),
                        address,
                        value,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_write_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::WriteSingleCoil, e)
                                }
                                _ => ModbusResponse::echo_coil(address, value),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::WriteSingleCoil,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::WriteSingleCoil,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::echo_coil(address, value)
                }
            }
            #[cfg(feature = "coils")]
            ModbusRequest::WriteMultipleCoils {
                address,
                count,
                data,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_write_multiple_coils {
                    let mut values = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        let byte_idx = (i / 8) as usize;
                        let bit_idx = i % 8;
                        if byte_idx < data.len() {
                            values.push((data[byte_idx] & (1 << bit_idx)) != 0);
                        } else {
                            values.push(false);
                        }
                    }
                    let js_req = WriteMultipleCoilsRequest {
                        unit_id: u8::from(unit),
                        address,
                        values,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_write_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::WriteMultipleCoils, e)
                                }
                                _ => ModbusResponse::echo_multi_write(
                                    FunctionCode::WriteMultipleCoils,
                                    address,
                                    count,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::WriteMultipleCoils,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::WriteMultipleCoils,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::echo_multi_write(
                        FunctionCode::WriteMultipleCoils,
                        address,
                        count,
                    )
                }
            }
            #[cfg(feature = "discrete-inputs")]
            ModbusRequest::ReadDiscreteInputs {
                address,
                count,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_read_discrete_inputs {
                    let js_req = ReadDiscreteInputsRequest {
                        unit_id: u8::from(unit),
                        address,
                        quantity: count,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_bool_vec_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::ReadDiscreteInputs, e)
                                }
                                HandlerReturn::Data(bits) => {
                                    if bits.len() == count as usize {
                                        ModbusResponse::packed_bits(
                                            FunctionCode::ReadDiscreteInputs,
                                            &pack_bits(&bits),
                                        )
                                    } else {
                                        ModbusResponse::exception(
                                            FunctionCode::ReadDiscreteInputs,
                                            ExceptionCode::IllegalDataAddress,
                                        )
                                    }
                                }
                                HandlerReturn::Void => ModbusResponse::exception(
                                    FunctionCode::ReadDiscreteInputs,
                                    ExceptionCode::ServerDeviceFailure,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::ReadDiscreteInputs,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::ReadDiscreteInputs,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadDiscreteInputs,
                        ExceptionCode::IllegalFunction,
                    )
                }
            }
            #[cfg(feature = "holding-registers")]
            ModbusRequest::ReadHoldingRegisters {
                address,
                count,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_read_holding_registers {
                    let js_req = ReadHoldingRegistersRequest {
                        unit_id: u8::from(unit),
                        address,
                        quantity: count,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_u16_vec_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::ReadHoldingRegisters, e)
                                }
                                HandlerReturn::Data(regs) => {
                                    if regs.len() == count as usize {
                                        ModbusResponse::registers(
                                            FunctionCode::ReadHoldingRegisters,
                                            &regs,
                                        )
                                    } else {
                                        ModbusResponse::exception(
                                            FunctionCode::ReadHoldingRegisters,
                                            ExceptionCode::IllegalDataAddress,
                                        )
                                    }
                                }
                                HandlerReturn::Void => ModbusResponse::exception(
                                    FunctionCode::ReadHoldingRegisters,
                                    ExceptionCode::ServerDeviceFailure,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::ReadHoldingRegisters,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::ReadHoldingRegisters,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadHoldingRegisters,
                        ExceptionCode::IllegalFunction,
                    )
                }
            }
            #[cfg(feature = "input-registers")]
            ModbusRequest::ReadInputRegisters {
                address,
                count,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_read_input_registers {
                    let js_req = ReadInputRegistersRequest {
                        unit_id: u8::from(unit),
                        address,
                        quantity: count,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_u16_vec_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::ReadInputRegisters, e)
                                }
                                HandlerReturn::Data(regs) => {
                                    if regs.len() == count as usize {
                                        ModbusResponse::registers(
                                            FunctionCode::ReadInputRegisters,
                                            &regs,
                                        )
                                    } else {
                                        ModbusResponse::exception(
                                            FunctionCode::ReadInputRegisters,
                                            ExceptionCode::IllegalDataAddress,
                                        )
                                    }
                                }
                                HandlerReturn::Void => ModbusResponse::exception(
                                    FunctionCode::ReadInputRegisters,
                                    ExceptionCode::ServerDeviceFailure,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::ReadInputRegisters,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::ReadInputRegisters,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadInputRegisters,
                        ExceptionCode::IllegalFunction,
                    )
                }
            }
            #[cfg(feature = "holding-registers")]
            ModbusRequest::WriteSingleRegister {
                address,
                value,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_write_single_register {
                    let js_req = WriteSingleRegisterRequest {
                        unit_id: u8::from(unit),
                        address,
                        value,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_write_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::WriteSingleRegister, e)
                                }
                                _ => ModbusResponse::echo_register(address, value),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::WriteSingleRegister,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::WriteSingleRegister,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::echo_register(address, value)
                }
            }
            #[cfg(feature = "holding-registers")]
            ModbusRequest::WriteMultipleRegisters {
                address,
                count,
                data,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_write_multiple_registers {
                    let mut values = Vec::with_capacity(count as usize);
                    for i in 0..count {
                        let idx = (i * 2) as usize;
                        if idx + 1 < data.len() {
                            values.push(u16::from_be_bytes([data[idx], data[idx + 1]]));
                        }
                    }
                    let js_req = WriteMultipleRegistersRequest {
                        unit_id: u8::from(unit),
                        address,
                        values,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_write_return(val) {
                                HandlerReturn::Exception(e) => ModbusResponse::exception(
                                    FunctionCode::WriteMultipleRegisters,
                                    e,
                                ),
                                _ => ModbusResponse::echo_multi_write(
                                    FunctionCode::WriteMultipleRegisters,
                                    address,
                                    count,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::WriteMultipleRegisters,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::WriteMultipleRegisters,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::echo_multi_write(
                        FunctionCode::WriteMultipleRegisters,
                        address,
                        count,
                    )
                }
            }
            #[cfg(feature = "holding-registers")]
            ModbusRequest::MaskWriteRegister { .. } => ModbusResponse::exception(
                FunctionCode::MaskWriteRegister,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "holding-registers")]
            ModbusRequest::ReadWriteMultipleRegisters { .. } => ModbusResponse::exception(
                FunctionCode::ReadWriteMultipleRegisters,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "fifo")]
            ModbusRequest::ReadFifoQueue {
                pointer_address,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_read_fifo_queue {
                    let js_req = ReadFifoQueueRequest {
                        unit_id: u8::from(unit),
                        address: pointer_address,
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_u16_vec_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::ReadFifoQueue, e)
                                }
                                HandlerReturn::Data(values) => {
                                    if values.len() <= 31 {
                                        let fifo_count = values.len() as u16;
                                        let mut payload = heapless::Vec::<
                                            u8,
                                            { mbus_core::data_unit::common::MAX_PDU_DATA_LEN },
                                        >::new(
                                        );
                                        let _ =
                                            payload.extend_from_slice(&fifo_count.to_be_bytes());
                                        for v in &values {
                                            let _ = payload.extend_from_slice(&v.to_be_bytes());
                                        }
                                        ModbusResponse::fifo_response(&payload)
                                    } else {
                                        ModbusResponse::exception(
                                            FunctionCode::ReadFifoQueue,
                                            ExceptionCode::IllegalDataAddress,
                                        )
                                    }
                                }
                                HandlerReturn::Void => ModbusResponse::exception(
                                    FunctionCode::ReadFifoQueue,
                                    ExceptionCode::ServerDeviceFailure,
                                ),
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::ReadFifoQueue,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::ReadFifoQueue,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::exception(
                        FunctionCode::ReadFifoQueue,
                        ExceptionCode::IllegalFunction,
                    )
                }
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReadExceptionStatus { .. } => ModbusResponse::read_exception_status(0),
            #[cfg(feature = "diagnostics")]
            ModbusRequest::Diagnostics {
                sub_function,
                data,
                unit,
                ..
            } => {
                if let Some(handler) = &self.on_diagnostics {
                    let js_req = DiagnosticsRequest {
                        unit_id: u8::from(unit),
                        sub_function,
                        data: vec![data],
                    };
                    match handler.call_async(js_req).await {
                        Ok(promise) => match promise.await {
                            Ok(val) => match parse_diagnostics_return(val) {
                                HandlerReturn::Exception(e) => {
                                    ModbusResponse::exception(FunctionCode::Diagnostics, e)
                                }
                                HandlerReturn::Data(res) => {
                                    let resp_data = res.data.first().copied().unwrap_or(0);
                                    ModbusResponse::diagnostics_echo(res.sub_function, resp_data)
                                }
                                HandlerReturn::Void => {
                                    ModbusResponse::diagnostics_echo(sub_function, data)
                                }
                            },
                            Err(_) => ModbusResponse::exception(
                                FunctionCode::Diagnostics,
                                ExceptionCode::ServerDeviceFailure,
                            ),
                        },
                        Err(_) => ModbusResponse::exception(
                            FunctionCode::Diagnostics,
                            ExceptionCode::ServerDeviceFailure,
                        ),
                    }
                } else {
                    ModbusResponse::diagnostics_echo(sub_function, data)
                }
            }
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventCounter { .. } => ModbusResponse::comm_event_counter(0, 0),
            #[cfg(feature = "diagnostics")]
            ModbusRequest::GetCommEventLog { .. } => ModbusResponse::exception(
                FunctionCode::GetCommEventLog,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "diagnostics")]
            ModbusRequest::ReportServerId { .. } => ModbusResponse::exception(
                FunctionCode::ReportServerId,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "file-record")]
            ModbusRequest::ReadFileRecord { .. } => ModbusResponse::exception(
                FunctionCode::ReadFileRecord,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "file-record")]
            ModbusRequest::WriteFileRecord { .. } => ModbusResponse::exception(
                FunctionCode::WriteFileRecord,
                ExceptionCode::IllegalFunction,
            ),
            #[cfg(feature = "diagnostics")]
            ModbusRequest::EncapsulatedInterfaceTransport { .. } => {
                ModbusResponse::exception_raw(0x2B, ExceptionCode::IllegalFunction)
            }
            _ => ModbusResponse::NoResponse,
        }
    }
}

// ── Build adapter helper ─────────────────────────────────────────────────────

/// Helper to build JsHandlerAdapter from handlers Object.
pub fn build_adapter(env: &Env, handlers: &Object) -> Result<JsHandlerAdapter> {
    // Wrap all handler functions so they always return a Promise.
    // This lets users write either sync or async handlers seamlessly.
    let wrapped = wrap_handlers_to_promise(env, handlers)?;

    macro_rules! get_handler {
        ($name:expr, $req_type:ty) => {
            wrapped.get::<JsHandler<$req_type>>($name)?.map(Arc::new)
        };
    }

    Ok(JsHandlerAdapter {
        #[cfg(feature = "coils")]
        on_read_coils: get_handler!("onReadCoils", ReadCoilsRequest),
        #[cfg(feature = "coils")]
        on_write_single_coil: get_handler!("onWriteSingleCoil", WriteSingleCoilRequest),
        #[cfg(feature = "coils")]
        on_write_multiple_coils: get_handler!("onWriteMultipleCoils", WriteMultipleCoilsRequest),
        #[cfg(feature = "discrete-inputs")]
        on_read_discrete_inputs: get_handler!("onReadDiscreteInputs", ReadDiscreteInputsRequest),
        #[cfg(feature = "holding-registers")]
        on_read_holding_registers: get_handler!(
            "onReadHoldingRegisters",
            ReadHoldingRegistersRequest
        ),
        #[cfg(feature = "input-registers")]
        on_read_input_registers: get_handler!("onReadInputRegisters", ReadInputRegistersRequest),
        #[cfg(feature = "holding-registers")]
        on_write_single_register: get_handler!("onWriteSingleRegister", WriteSingleRegisterRequest),
        #[cfg(feature = "holding-registers")]
        on_write_multiple_registers: get_handler!(
            "onWriteMultipleRegisters",
            WriteMultipleRegistersRequest
        ),
        #[cfg(feature = "fifo")]
        on_read_fifo_queue: get_handler!("onReadFifoQueue", ReadFifoQueueRequest),
        #[cfg(feature = "diagnostics")]
        on_diagnostics: get_handler!("onDiagnostics", DiagnosticsRequest),
    })
}

/// Wraps each function property of `handlers` with `Promise.resolve()`.
///
/// Returns a new JS object where every function `fn` has been replaced with
/// `(...args) => Promise.resolve(fn.apply(undefined, args))`.
///
/// This is necessary because `JsHandler` expects a `Promise` return type,
/// but users may supply synchronous handler functions.
fn wrap_handlers_to_promise<'a>(env: &'a Env, handlers: &'a Object<'a>) -> Result<Object<'a>> {
    let mut global = env.get_global()?;
    global.set_named_property("__mbus_handlers_tmp", handlers)?;

    let wrapped: Object<'_> = env.run_script(
        r#"(function() {
            var h = globalThis.__mbus_handlers_tmp;
            var w = {};
            Object.keys(h).forEach(function(k) {
                if (typeof h[k] === 'function') {
                    w[k] = (function(fn) {
                        return function() {
                            return Promise.resolve(fn.apply(undefined, arguments));
                        };
                    })(h[k]);
                }
            });
            delete globalThis.__mbus_handlers_tmp;
            return w;
        })()"#,
    )?;

    Ok(wrapped)
}

// ── AsyncTcpModbusServer ─────────────────────────────────────────────────────

/// Async Modbus TCP server.
///
/// Binds to a TCP port and handles incoming Modbus requests using JS callbacks.
#[napi]
pub struct AsyncTcpModbusServer {
    stop_signal: Arc<Notify>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

#[napi]
impl AsyncTcpModbusServer {
    /// Creates and starts a new TCP server.
    ///
    /// @param opts - Server bind options.
    /// @param handlers - Object containing handler functions for each Modbus operation.
    /// @returns A running server instance.
    #[napi(factory)]
    pub fn bind(
        env: Env,
        opts: TcpServerOptions,
        handlers: Object,
    ) -> Result<AsyncTcpModbusServer> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let bind_addr = format!("{}:{}", opts.host, opts.port);
        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the handler adapter
        let adapter = build_adapter(&env, &handlers)?;

        // Spawn the server task
        let rt = runtime::get();
        let join_handle = rt.spawn(async move {
            let _ = AsyncTcpServer::serve_with_shutdown(
                &bind_addr,
                adapter,
                unit,
                stop_signal_clone.notified(),
            )
            .await;
        });

        Ok(AsyncTcpModbusServer {
            stop_signal,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    /// Stops the server.
    #[napi]
    pub async fn shutdown(&self) -> Result<()> {
        self.stop_signal.notify_one();

        let handle = {
            let mut guard = self
                .join_handle
                .lock()
                .map_err(|_| napi::Error::new(Status::GenericFailure, "Failed to acquire lock"))?;
            guard.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }

        Ok(())
    }
}
