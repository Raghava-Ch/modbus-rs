use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
use mbus_server_async::{AsyncAppHandler, ModbusRequest, ModbusResponse};
use pyo3::exceptions::{PyIndexError, PyNotImplementedError, PyValueError};
use pyo3::prelude::*;
use std::future::Future;
use std::sync::Arc;

// ── Python base class ────────────────────────────────────────────────────────

/// Base class for a Modbus server application.
///
/// Subclass this and override the ``handle_*`` methods for the function codes
/// your device supports.  Any method that is not overridden returns an
/// ``IllegalFunction`` exception to the Modbus client.
///
/// Example::
///
/// ```python
/// import modbus_rs
///
/// class MyApp(modbus_rs.ModbusApp):
///     def handle_read_holding_registers(self, address, count):
///         return [address + i for i in range(count)]
///
///     def handle_write_register(self, address, value):
///         pass  # store value
/// ```
#[pyclass(name = "ModbusApp", subclass)]
pub struct ModbusApp;

#[pymethods]
impl ModbusApp {
    #[new]
    fn new() -> Self {
        ModbusApp
    }

    // ── FC01 / FC05 / FC0F — Coils ──────────────────────────────────────────

    /// Handle FC01 Read Coils. Returns ``list[bool]``.
    #[pyo3(signature = (address, count))]
    fn handle_read_coils(&self, address: u16, count: u16) -> PyResult<Vec<bool>> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_read_coils(address={address}, count={count}) not implemented"
        )))
    }

    /// Handle FC05 Write Single Coil.
    #[pyo3(signature = (address, value))]
    fn handle_write_coil(&self, address: u16, value: bool) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_coil(address={address}, value={value}) not implemented"
        )))
    }

    /// Handle FC0F Write Multiple Coils. ``values`` is a list of booleans.
    #[pyo3(signature = (address, values))]
    fn handle_write_coils(&self, address: u16, values: Vec<bool>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_coils(address={address}, values=<{} items>) not implemented",
            values.len()
        )))
    }

    // ── FC02 — Discrete Inputs ──────────────────────────────────────────────

    /// Handle FC02 Read Discrete Inputs. Returns ``list[bool]``.
    #[pyo3(signature = (address, count))]
    fn handle_read_discrete_inputs(&self, address: u16, count: u16) -> PyResult<Vec<bool>> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_read_discrete_inputs(address={address}, count={count}) not implemented"
        )))
    }

    // ── FC03 / FC04 / FC06 / FC10 / FC16 / FC17 — Registers ────────────────

    /// Handle FC03 Read Holding Registers. Returns ``list[int]``.
    #[pyo3(signature = (address, count))]
    fn handle_read_holding_registers(&self, address: u16, count: u16) -> PyResult<Vec<u16>> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_read_holding_registers(address={address}, count={count}) not implemented"
        )))
    }

    /// Handle FC04 Read Input Registers. Returns ``list[int]``.
    #[pyo3(signature = (address, count))]
    fn handle_read_input_registers(&self, address: u16, count: u16) -> PyResult<Vec<u16>> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_read_input_registers(address={address}, count={count}) not implemented"
        )))
    }

    /// Handle FC06 Write Single Register.
    #[pyo3(signature = (address, value))]
    fn handle_write_register(&self, address: u16, value: u16) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_register(address={address}, value={value}) not implemented"
        )))
    }

    /// Handle FC10 Write Multiple Registers. ``values`` is a list of holding registers.
    #[pyo3(signature = (address, values))]
    fn handle_write_registers(&self, address: u16, values: Vec<u16>) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_registers(address={address}, values=<{} items>) not implemented",
            values.len()
        )))
    }

    /// Handle FC16 Mask Write Register.
    #[pyo3(signature = (address, and_mask, or_mask))]
    fn handle_mask_write_register(
        &self,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_mask_write_register(address={address}, and_mask={and_mask}, or_mask={or_mask}) not implemented"
        )))
    }

    /// Handle FC17 Read/Write Multiple Registers.
    /// ``write_values`` is a list of holding registers to write. Returns ``list[int]``.
    #[pyo3(signature = (read_address, read_count, write_address, write_values))]
    fn handle_read_write_registers(
        &self,
        read_address: u16,
        read_count: u16,
        write_address: u16,
        write_values: Vec<u16>,
    ) -> PyResult<Vec<u16>> {
        let _ = (read_address, read_count, write_address, write_values);
        Err(PyNotImplementedError::new_err(
            "handle_read_write_registers not implemented",
        ))
    }

    // ── FC18 — FIFO ─────────────────────────────────────────────────────────

    /// Handle FC18 Read FIFO Queue. Returns ``list[int]`` (≤31 values).
    #[pyo3(signature = (pointer_address))]
    fn handle_read_fifo_queue(&self, pointer_address: u16) -> PyResult<Vec<u16>> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_read_fifo_queue(pointer_address={pointer_address}) not implemented"
        )))
    }

    // ── Diagnostics ─────────────────────────────────────────────────────────

    /// Handle FC07 Read Exception Status. Returns ``int`` status byte.
    fn handle_read_exception_status(&self) -> PyResult<u8> {
        Err(PyNotImplementedError::new_err(
            "handle_read_exception_status() not implemented",
        ))
    }

    /// Handle FC0B Get Comm Event Counter. Returns ``(status_word, event_count)``.
    fn handle_get_comm_event_counter(&self) -> PyResult<(u16, u16)> {
        Err(PyNotImplementedError::new_err(
            "handle_get_comm_event_counter() not implemented",
        ))
    }

    /// Handle FC08 Diagnostics. Returns ``(sub_function, result)`` to echo.
    #[pyo3(signature = (sub_function, data))]
    fn handle_diagnostics(&self, sub_function: u16, data: u16) -> PyResult<(u16, u16)> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_diagnostics(sub_function={sub_function}, data={data}) not implemented"
        )))
    }

    /// Handle FC0C Get Comm Event Log. Return byte payload.
    fn handle_get_comm_event_log(&self) -> PyResult<Vec<u8>> {
        Err(PyNotImplementedError::new_err(
            "handle_get_comm_event_log() not implemented",
        ))
    }

    /// Handle FC11 Report Server ID. Return byte payload.
    fn handle_report_server_id(&self) -> PyResult<Vec<u8>> {
        Err(PyNotImplementedError::new_err(
            "handle_report_server_id() not implemented",
        ))
    }

    /// Lifecycle hook fired when an exception response is sent.
    ///
    /// Default implementation is a no-op. Override in Python to log or
    /// observe exception traffic.
    #[pyo3(signature = (txn_id, unit_id, function_code, exception_code))]
    fn on_exception(
        &self,
        txn_id: u16,
        unit_id: u8,
        function_code: u8,
        exception_code: u8,
    ) -> PyResult<()> {
        let _ = (txn_id, unit_id, function_code, exception_code);
        Ok(())
    }
}

// ── Rust adapter implementing AsyncAppHandler ────────────────────────────────

/// Wraps a Python `ModbusApp` subclass and implements `AsyncAppHandler`.
#[derive(Clone)]
pub struct PythonAppAdapter {
    pub app: Arc<Py<ModbusApp>>,
    pub event_loop: Option<Arc<Py<PyAny>>>,
}

impl PythonAppAdapter {
    pub fn new(py_app: Py<ModbusApp>, event_loop: Option<Py<PyAny>>) -> Self {
        Self {
            app: Arc::new(py_app),
            event_loop: event_loop.map(Arc::new),
        }
    }
}

// When the `traffic` feature is enabled in the workspace build, `AsyncAppHandler`
// requires `AsyncTrafficNotifier` as a super-trait. Provide an empty default
// impl so the Python adapter remains usable in that build matrix.
#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncServerTrafficNotifier for PythonAppAdapter {}

impl AsyncAppHandler for PythonAppAdapter {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let app = self.app.clone();
        let event_loop = self.event_loop.clone();
        async move { dispatch_request(app, event_loop, req).await }
    }

    fn on_exception(
        &mut self,
        txn_id: u16,
        unit: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        exception_code: ExceptionCode,
    ) {
        let app = self.app.clone();

        Python::attach(|py| {
            if let Err(err) = app.call_method1(
                py,
                "on_exception",
                (
                    txn_id,
                    u8::from(unit),
                    function_code as u8,
                    u8::from(exception_code),
                ),
            ) {
                err.print(py);
            }
        });
    }
}

// ── dispatch helper ──────────────────────────────────────────────────────────

fn py_error_to_response(py: Python<'_>, fc: FunctionCode, err: PyErr) -> ModbusResponse {
    err.print(py);
    if err.is_instance_of::<PyNotImplementedError>(py) {
        return ModbusResponse::exception(fc, ExceptionCode::IllegalFunction);
    }

    if err.is_instance_of::<PyValueError>(py) || err.is_instance_of::<PyIndexError>(py) {
        return ModbusResponse::exception(fc, ExceptionCode::IllegalDataAddress);
    }

    ModbusResponse::exception(fc, ExceptionCode::ServerDeviceFailure)
}

fn invalid_function_for_fc_byte(fc_byte: u8) -> ModbusResponse {
    ModbusResponse::exception_raw(fc_byte, ExceptionCode::IllegalFunction)
}

async fn eval_python_call_global<F>(
    event_loop: Option<Arc<Py<PyAny>>>,
    call: F,
) -> PyResult<Py<PyAny>>
where
    F: for<'py> FnOnce(Python<'py>) -> PyResult<Bound<'py, PyAny>>,
{
    let (is_awaitable, obj) = Python::attach(|py| {
        let val = call(py)?;
        let is_await = val.hasattr("__await__").unwrap_or(false);
        Ok::<_, PyErr>((is_await, val.unbind()))
    })?;

    if is_awaitable {
        let loop_obj = event_loop.ok_or_else(|| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(
                "Async handlers are only supported when running within an active asyncio event loop."
            )
        })?;
        let fut = Python::attach(|py| {
            let loop_bound = loop_obj.bind(py);
            let locals = pyo3_async_runtimes::TaskLocals::new(loop_bound.clone());
            pyo3_async_runtimes::into_future_with_locals(&locals, obj.bind(py).clone())
        })?;
        let res = fut.await?;
        Ok(res)
    } else {
        Ok(obj)
    }
}

async fn dispatch_request(
    app: Arc<Py<ModbusApp>>,
    event_loop: Option<Arc<Py<PyAny>>>,
    req: ModbusRequest,
) -> ModbusResponse {
    macro_rules! eval_python_call {
        ($call:expr) => {
            eval_python_call_global(event_loop.clone(), $call)
        };
    }

    match req {
        // FC01 — Read Coils
        #[cfg(feature = "coils")]
        ModbusRequest::ReadCoils { address, count, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_read_coils", (address, count))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<bool>>()) {
                    Ok(bits) if bits.len() == count as usize => {
                        let mut packed = heapless::Vec::<
                            u8,
                            { mbus_core::data_unit::common::MAX_PDU_DATA_LEN },
                        >::new();
                        for chunk in bits.chunks(8) {
                            let mut byte: u8 = 0;
                            for (i, &b) in chunk.iter().enumerate() {
                                if b {
                                    byte |= 1 << i;
                                }
                            }
                            let _ = packed.push(byte);
                        }
                        ModbusResponse::packed_bits(FunctionCode::ReadCoils, &packed)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadCoils,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadCoils, e),
                },
            )
        }

        // FC05 — Write Single Coil
        #[cfg(feature = "coils")]
        ModbusRequest::WriteSingleCoil { address, value, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_write_coil", (address, value))
            })
            .await;
            Python::attach(|py| match result {
                Ok(_) => ModbusResponse::echo_coil(address, value),
                Err(e) => py_error_to_response(py, FunctionCode::WriteSingleCoil, e),
            })
        }

        // FC0F — Write Multiple Coils
        #[cfg(feature = "coils")]
        ModbusRequest::WriteMultipleCoils {
            address,
            count,
            data,
            ..
        } => {
            let mut values = Vec::with_capacity(count as usize);
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
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_write_coils", (address, values))
            })
            .await;
            Python::attach(|py| match result {
                Ok(_) => ModbusResponse::echo_multi_write(
                    FunctionCode::WriteMultipleCoils,
                    address,
                    count,
                ),
                Err(e) => py_error_to_response(py, FunctionCode::WriteMultipleCoils, e),
            })
        }

        // FC02 — Read Discrete Inputs
        #[cfg(feature = "discrete-inputs")]
        ModbusRequest::ReadDiscreteInputs { address, count, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_read_discrete_inputs", (address, count))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<bool>>()) {
                    Ok(bits) if bits.len() == count as usize => {
                        let mut packed = heapless::Vec::<
                            u8,
                            { mbus_core::data_unit::common::MAX_PDU_DATA_LEN },
                        >::new();
                        for chunk in bits.chunks(8) {
                            let mut byte: u8 = 0;
                            for (i, &b) in chunk.iter().enumerate() {
                                if b {
                                    byte |= 1 << i;
                                }
                            }
                            let _ = packed.push(byte);
                        }
                        ModbusResponse::packed_bits(FunctionCode::ReadDiscreteInputs, &packed)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadDiscreteInputs,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadDiscreteInputs, e),
                },
            )
        }

        // FC03 — Read Holding Registers
        #[cfg(feature = "holding-registers")]
        ModbusRequest::ReadHoldingRegisters { address, count, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_read_holding_registers", (address, count))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u16>>()) {
                    Ok(regs) if regs.len() == count as usize => {
                        ModbusResponse::registers(FunctionCode::ReadHoldingRegisters, &regs)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadHoldingRegisters,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadHoldingRegisters, e),
                },
            )
        }

        // FC04 — Read Input Registers
        #[cfg(feature = "input-registers")]
        ModbusRequest::ReadInputRegisters { address, count, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_read_input_registers", (address, count))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u16>>()) {
                    Ok(regs) if regs.len() == count as usize => {
                        ModbusResponse::registers(FunctionCode::ReadInputRegisters, &regs)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadInputRegisters,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadInputRegisters, e),
                },
            )
        }

        // FC06 — Write Single Register
        #[cfg(feature = "holding-registers")]
        ModbusRequest::WriteSingleRegister { address, value, .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_write_register", (address, value))
            })
            .await;
            Python::attach(|py| match result {
                Ok(_) => ModbusResponse::echo_register(address, value),
                Err(e) => py_error_to_response(py, FunctionCode::WriteSingleRegister, e),
            })
        }

        // FC10 — Write Multiple Registers
        #[cfg(feature = "holding-registers")]
        ModbusRequest::WriteMultipleRegisters {
            address,
            count,
            data,
            ..
        } => {
            let mut values = Vec::with_capacity(count as usize);
            for chunk in data.chunks(2) {
                if chunk.len() == 2 {
                    values.push(u16::from_be_bytes([chunk[0], chunk[1]]));
                }
            }
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_write_registers", (address, values))
            })
            .await;
            Python::attach(|py| match result {
                Ok(_) => ModbusResponse::echo_multi_write(
                    FunctionCode::WriteMultipleRegisters,
                    address,
                    count,
                ),
                Err(e) => py_error_to_response(py, FunctionCode::WriteMultipleRegisters, e),
            })
        }

        // FC16 — Mask Write Register
        #[cfg(feature = "holding-registers")]
        ModbusRequest::MaskWriteRegister {
            address,
            and_mask,
            or_mask,
            ..
        } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_mask_write_register", (address, and_mask, or_mask))
            })
            .await;
            Python::attach(|py| match result {
                Ok(_) => ModbusResponse::echo_mask_write(address, and_mask, or_mask),
                Err(e) => py_error_to_response(py, FunctionCode::MaskWriteRegister, e),
            })
        }

        // FC17 — Read/Write Multiple Registers
        #[cfg(feature = "holding-registers")]
        ModbusRequest::ReadWriteMultipleRegisters {
            read_address,
            read_count,
            write_address,
            write_count: _write_count,
            data,
            ..
        } => {
            let mut write_values = Vec::with_capacity(data.len() / 2);
            for chunk in data.chunks(2) {
                if chunk.len() == 2 {
                    write_values.push(u16::from_be_bytes([chunk[0], chunk[1]]));
                }
            }
            let result = eval_python_call!(|py| {
                app.bind(py).call_method1(
                    "handle_read_write_registers",
                    (read_address, read_count, write_address, write_values),
                )
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u16>>()) {
                    Ok(regs) if regs.len() == read_count as usize => {
                        ModbusResponse::registers(FunctionCode::ReadWriteMultipleRegisters, &regs)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadWriteMultipleRegisters,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadWriteMultipleRegisters, e),
                },
            )
        }

        // FC18 — Read FIFO Queue
        #[cfg(feature = "fifo")]
        ModbusRequest::ReadFifoQueue {
            pointer_address, ..
        } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_read_fifo_queue", (pointer_address,))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u16>>()) {
                    Ok(values) if values.len() <= 31 => {
                        // Payload: fifo_count (2 BE) + values (2 BE each)
                        let fifo_count = values.len() as u16;
                        let mut payload = heapless::Vec::<
                            u8,
                            { mbus_core::data_unit::common::MAX_PDU_DATA_LEN },
                        >::new();
                        let _ = payload.extend_from_slice(&fifo_count.to_be_bytes());
                        for v in &values {
                            let _ = payload.extend_from_slice(&v.to_be_bytes());
                        }
                        ModbusResponse::fifo_response(&payload)
                    }
                    Ok(_) => ModbusResponse::exception(
                        FunctionCode::ReadFifoQueue,
                        ExceptionCode::IllegalDataAddress,
                    ),
                    Err(e) => py_error_to_response(py, FunctionCode::ReadFifoQueue, e),
                },
            )
        }

        // FC07 — Read Exception Status
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReadExceptionStatus { .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py).call_method0("handle_read_exception_status")
            })
            .await;
            Python::attach(|py| match result.and_then(|v| v.bind(py).extract::<u8>()) {
                Ok(status) => ModbusResponse::read_exception_status(status),
                Err(e) => py_error_to_response(py, FunctionCode::ReadExceptionStatus, e),
            })
        }

        // FC08 — Diagnostics
        #[cfg(feature = "diagnostics")]
        ModbusRequest::Diagnostics {
            sub_function, data, ..
        } => {
            let result = eval_python_call!(|py| {
                app.bind(py)
                    .call_method1("handle_diagnostics", (sub_function, data))
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<(u16, u16)>()) {
                    Ok((response_sub_fn, response_data)) => {
                        ModbusResponse::diagnostics_echo(response_sub_fn, response_data)
                    }
                    Err(e) if e.is_instance_of::<PyNotImplementedError>(py) => {
                        ModbusResponse::diagnostics_echo(sub_function, data)
                    }
                    Err(e) => py_error_to_response(py, FunctionCode::Diagnostics, e),
                },
            )
        }

        // FC0B — Get Comm Event Counter
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventCounter { .. } => {
            let result = eval_python_call!(|py| {
                app.bind(py).call_method0("handle_get_comm_event_counter")
            })
            .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<(u16, u16)>()) {
                    Ok((status, count)) => ModbusResponse::comm_event_counter(status, count),
                    Err(e) => py_error_to_response(py, FunctionCode::GetCommEventCounter, e),
                },
            )
        }

        // FC0C — Get Comm Event Log
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventLog { .. } => {
            let result =
                eval_python_call!(|py| { app.bind(py).call_method0("handle_get_comm_event_log") })
                    .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u8>>()) {
                    Ok(payload) => ModbusResponse::comm_event_log(&payload),
                    Err(e) => py_error_to_response(py, FunctionCode::GetCommEventLog, e),
                },
            )
        }

        // FC11 — Report Server ID
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReportServerId { .. } => {
            let result =
                eval_python_call!(|py| { app.bind(py).call_method0("handle_report_server_id") })
                    .await;
            Python::attach(
                |py| match result.and_then(|v| v.bind(py).extract::<Vec<u8>>()) {
                    Ok(payload) => ModbusResponse::report_server_id(&payload),
                    Err(e) => py_error_to_response(py, FunctionCode::ReportServerId, e),
                },
            )
        }

        // All other variants → IllegalFunction
        other => {
            let fc_byte = other.function_code_byte();
            invalid_function_for_fc_byte(fc_byte)
        }
    }
}
