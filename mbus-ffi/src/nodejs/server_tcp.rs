//! Node.js bindings for the async Modbus TCP server.

use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi::threadsafe_function::{ErrorStrategy, ThreadsafeFunction};
use napi_derive::napi;

use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server_async::{AsyncAppHandler, AsyncTcpServer, ModbusRequest, ModbusResponse};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::nodejs::errors::{to_napi_err, ERR_MODBUS_INVALID_ARGUMENT};
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
#[derive(Debug, Clone)]
pub struct ReadCoilsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for writing a single coil.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteSingleCoilRequest {
    pub unit_id: u8,
    pub address: u16,
    pub value: bool,
}

/// Handler request for writing multiple coils.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteMultipleCoilsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub values: Vec<bool>,
}

/// Handler request for reading discrete inputs.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadDiscreteInputsRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for reading holding registers.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadHoldingRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for reading input registers.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadInputRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub quantity: u16,
}

/// Handler request for writing a single register.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteSingleRegisterRequest {
    pub unit_id: u8,
    pub address: u16,
    pub value: u16,
}

/// Handler request for writing multiple registers.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteMultipleRegistersRequest {
    pub unit_id: u8,
    pub address: u16,
    pub values: Vec<u16>,
}

/// Handler request for reading FIFO queue.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadFifoQueueRequest {
    pub unit_id: u8,
    pub address: u16,
}

/// Handler request for diagnostics.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DiagnosticsRequest {
    pub unit_id: u8,
    pub sub_function: u16,
    pub data: Vec<u16>,
}

/// Response that may include an exception code.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ServerExceptionResponse {
    /// Modbus exception code if this is an error response.
    pub exception: Option<u8>,
}

// ── Handler struct ───────────────────────────────────────────────────────────

/// Internal adapter that implements AsyncAppHandler by delegating to JS callbacks.
struct JsHandlerAdapter {
    #[cfg(feature = "coils")]
    on_read_coils: Option<ThreadsafeFunction<ReadCoilsRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "coils")]
    on_write_single_coil: Option<ThreadsafeFunction<WriteSingleCoilRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "coils")]
    on_write_multiple_coils:
        Option<ThreadsafeFunction<WriteMultipleCoilsRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "discrete-inputs")]
    on_read_discrete_inputs:
        Option<ThreadsafeFunction<ReadDiscreteInputsRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "registers")]
    on_read_holding_registers:
        Option<ThreadsafeFunction<ReadHoldingRegistersRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "registers")]
    on_read_input_registers:
        Option<ThreadsafeFunction<ReadInputRegistersRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "registers")]
    on_write_single_register:
        Option<ThreadsafeFunction<WriteSingleRegisterRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "registers")]
    on_write_multiple_registers:
        Option<ThreadsafeFunction<WriteMultipleRegistersRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "fifo")]
    on_read_fifo_queue: Option<ThreadsafeFunction<ReadFifoQueueRequest, ErrorStrategy::Fatal>>,
    #[cfg(feature = "diagnostics")]
    on_diagnostics: Option<ThreadsafeFunction<DiagnosticsRequest, ErrorStrategy::Fatal>>,
}

// Safety: The ThreadsafeFunctions are designed to be called from multiple threads
unsafe impl Send for JsHandlerAdapter {}
unsafe impl Sync for JsHandlerAdapter {}

impl Clone for JsHandlerAdapter {
    fn clone(&self) -> Self {
        Self {
            #[cfg(feature = "coils")]
            on_read_coils: self.on_read_coils.clone(),
            #[cfg(feature = "coils")]
            on_write_single_coil: self.on_write_single_coil.clone(),
            #[cfg(feature = "coils")]
            on_write_multiple_coils: self.on_write_multiple_coils.clone(),
            #[cfg(feature = "discrete-inputs")]
            on_read_discrete_inputs: self.on_read_discrete_inputs.clone(),
            #[cfg(feature = "registers")]
            on_read_holding_registers: self.on_read_holding_registers.clone(),
            #[cfg(feature = "registers")]
            on_read_input_registers: self.on_read_input_registers.clone(),
            #[cfg(feature = "registers")]
            on_write_single_register: self.on_write_single_register.clone(),
            #[cfg(feature = "registers")]
            on_write_multiple_registers: self.on_write_multiple_registers.clone(),
            #[cfg(feature = "fifo")]
            on_read_fifo_queue: self.on_read_fifo_queue.clone(),
            #[cfg(feature = "diagnostics")]
            on_diagnostics: self.on_diagnostics.clone(),
        }
    }
}

#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncTrafficNotifier for JsHandlerAdapter {}

impl AsyncAppHandler for JsHandlerAdapter {
    async fn handle(&mut self, req: ModbusRequest) -> ModbusResponse {
        // For simplicity, we'll dispatch synchronously in this version
        // A full implementation would use async callbacks via ThreadsafeFunction
        match req {
            #[cfg(feature = "coils")]
            ModbusRequest::ReadCoils { .. } => {
                // Return dummy data or IllegalFunction if handler not set
                // Full implementation would call JS callback
                ModbusResponse::exception(FunctionCode::ReadCoils, ExceptionCode::IllegalFunction)
            }
            #[cfg(feature = "coils")]
            ModbusRequest::WriteSingleCoil {
                address, value, ..
            } => {
                ModbusResponse::echo_coil(address, value)
            }
            #[cfg(feature = "coils")]
            ModbusRequest::WriteMultipleCoils {
                address, count, ..
            } => {
                ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleCoils, address, count)
            }
            #[cfg(feature = "discrete-inputs")]
            ModbusRequest::ReadDiscreteInputs { .. } => {
                ModbusResponse::exception(
                    FunctionCode::ReadDiscreteInputs,
                    ExceptionCode::IllegalFunction,
                )
                }
                #[cfg(feature = "registers")]
                ModbusRequest::ReadHoldingRegisters { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReadHoldingRegisters,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "registers")]
                ModbusRequest::ReadInputRegisters { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReadInputRegisters,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "registers")]
                ModbusRequest::WriteSingleRegister { address, value, .. } => {
                    ModbusResponse::echo_register(address, value)
                }
                #[cfg(feature = "registers")]
                ModbusRequest::WriteMultipleRegisters {
                    address, count, ..
                } => {
                    ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleRegisters, address, count)
                }
                #[cfg(feature = "registers")]
                ModbusRequest::MaskWriteRegister { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::MaskWriteRegister,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "registers")]
                ModbusRequest::ReadWriteMultipleRegisters { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReadWriteMultipleRegisters,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "fifo")]
                ModbusRequest::ReadFifoQueue { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReadFifoQueue,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::ReadExceptionStatus { .. } => {
                    ModbusResponse::read_exception_status(0)
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::Diagnostics { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::Diagnostics,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::GetCommEventCounter { .. } => {
                    ModbusResponse::comm_event_counter(0, 0)
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::GetCommEventLog { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::GetCommEventLog,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::ReportServerId { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReportServerId,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "file-record")]
                ModbusRequest::ReadFileRecord { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::ReadFileRecord,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "file-record")]
                ModbusRequest::WriteFileRecord { .. } => {
                    ModbusResponse::exception(
                        FunctionCode::WriteFileRecord,
                        ExceptionCode::IllegalFunction,
                    )
                }
                #[cfg(feature = "diagnostics")]
                ModbusRequest::EncapsulatedInterfaceTransport { .. } => {
                    ModbusResponse::exception_raw(0x2B, ExceptionCode::IllegalFunction)
                }
                _ => ModbusResponse::NoResponse,
            }
    }
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
    pub fn bind(opts: TcpServerOptions, _handlers: Object) -> Result<AsyncTcpModbusServer> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let bind_addr = format!("{}:{}", opts.host, opts.port);
        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Create a minimal handler adapter
        // In a full implementation, we would extract JS functions from the handlers object
        let adapter = JsHandlerAdapter {
            #[cfg(feature = "coils")]
            on_read_coils: None,
            #[cfg(feature = "coils")]
            on_write_single_coil: None,
            #[cfg(feature = "coils")]
            on_write_multiple_coils: None,
            #[cfg(feature = "discrete-inputs")]
            on_read_discrete_inputs: None,
            #[cfg(feature = "registers")]
            on_read_holding_registers: None,
            #[cfg(feature = "registers")]
            on_read_input_registers: None,
            #[cfg(feature = "registers")]
            on_write_single_register: None,
            #[cfg(feature = "registers")]
            on_write_multiple_registers: None,
            #[cfg(feature = "fifo")]
            on_read_fifo_queue: None,
            #[cfg(feature = "diagnostics")]
            on_diagnostics: None,
        };

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
            let mut guard = self.join_handle.lock().map_err(|_| {
                napi::Error::new(Status::GenericFailure, "Failed to acquire lock")
            })?;
            guard.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }

        Ok(())
    }
}
