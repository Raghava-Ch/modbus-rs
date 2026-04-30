use mbus_server_async::{
    AsyncAppHandler, ModbusRequest, ModbusResponse,
};
use mbus_core::errors::ExceptionCode;
use mbus_core::function_codes::public::FunctionCode;
use mbus_core::transport::UnitIdOrSlaveAddr;
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

    /// Handle FC0F Write Multiple Coils. ``data`` is the packed coil bytes.
    #[pyo3(signature = (address, count, data))]
    fn handle_write_coils(&self, address: u16, count: u16, data: &[u8]) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_coils(address={address}, count={count}, data=<{} bytes>) not implemented",
            data.len()
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

    /// Handle FC10 Write Multiple Registers. ``data`` is big-endian 2-bytes/reg.
    #[pyo3(signature = (address, count, data))]
    fn handle_write_registers(&self, address: u16, count: u16, data: &[u8]) -> PyResult<()> {
        Err(PyNotImplementedError::new_err(format!(
            "handle_write_registers(address={address}, count={count}, data=<{} bytes>) not implemented",
            data.len()
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
    /// ``data`` is the write data (big-endian, 2 bytes/reg). Returns ``list[int]``.
    #[pyo3(signature = (read_address, read_count, write_address, write_count, data))]
    fn handle_read_write_registers(
        &self,
        read_address: u16,
        read_count: u16,
        write_address: u16,
        write_count: u16,
        data: &[u8],
    ) -> PyResult<Vec<u16>> {
        let _ = (read_address, read_count, write_address, write_count, data);
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
/// All calls are synchronous Python dispatch; the returned future is `ready`.
#[derive(Clone)]
pub struct PythonAppAdapter {
    pub app: Arc<Py<ModbusApp>>,
}

impl PythonAppAdapter {
    pub fn new(py_app: Py<ModbusApp>) -> Self {
        Self {
            app: Arc::new(py_app),
        }
    }
}

// When the `traffic` feature is enabled in the workspace build, `AsyncAppHandler`
// requires `AsyncTrafficNotifier` as a super-trait. Provide an empty default
// impl so the Python adapter remains usable in that build matrix.
#[cfg(feature = "traffic")]
impl mbus_server_async::AsyncTrafficNotifier for PythonAppAdapter {}

impl AsyncAppHandler for PythonAppAdapter {
    fn handle(&mut self, req: ModbusRequest) -> impl Future<Output = ModbusResponse> + Send {
        let app = self.app.clone();
        async move { Python::attach(|py| dispatch_request(py, app.as_ref(), req)) }
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

fn dispatch_request(py: Python<'_>, app: &Py<ModbusApp>, req: ModbusRequest) -> ModbusResponse {
    match req {
        // FC01 — Read Coils
        #[cfg(feature = "coils")]
        ModbusRequest::ReadCoils { address, count, .. } => {
            let result = app.call_method1(py, "handle_read_coils", (address, count));
            match result.and_then(|v| v.extract::<Vec<bool>>(py)) {
                Ok(bits) if bits.len() == count as usize => {
                    let mut packed = heapless::Vec::<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::new();
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
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadCoils, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadCoils, e),
            }
        }

        // FC05 — Write Single Coil
        #[cfg(feature = "coils")]
        ModbusRequest::WriteSingleCoil { address, value, .. } => {
            let result = app.call_method1(py, "handle_write_coil", (address, value));
            match result {
                Ok(_) => ModbusResponse::echo_coil(address, value),
                Err(e) => py_error_to_response(py, FunctionCode::WriteSingleCoil, e),
            }
        }

        // FC0F — Write Multiple Coils
        #[cfg(feature = "coils")]
        ModbusRequest::WriteMultipleCoils { address, count, data, .. } => {
            let result = app.call_method1(py, "handle_write_coils", (address, count, data.as_slice()));
            match result {
                Ok(_) => ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleCoils, address, count),
                Err(e) => py_error_to_response(py, FunctionCode::WriteMultipleCoils, e),
            }
        }

        // FC02 — Read Discrete Inputs
        #[cfg(feature = "discrete-inputs")]
        ModbusRequest::ReadDiscreteInputs { address, count, .. } => {
            let result = app.call_method1(py, "handle_read_discrete_inputs", (address, count));
            match result.and_then(|v| v.extract::<Vec<bool>>(py)) {
                Ok(bits) if bits.len() == count as usize => {
                    let mut packed = heapless::Vec::<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::new();
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
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadDiscreteInputs, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadDiscreteInputs, e),
            }
        }

        // FC03 — Read Holding Registers
        #[cfg(feature = "registers")]
        ModbusRequest::ReadHoldingRegisters { address, count, .. } => {
            let result = app.call_method1(py, "handle_read_holding_registers", (address, count));
            match result.and_then(|v| v.extract::<Vec<u16>>(py)) {
                Ok(regs) if regs.len() == count as usize => {
                    ModbusResponse::registers(FunctionCode::ReadHoldingRegisters, &regs)
                }
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadHoldingRegisters, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadHoldingRegisters, e),
            }
        }

        // FC04 — Read Input Registers
        #[cfg(feature = "registers")]
        ModbusRequest::ReadInputRegisters { address, count, .. } => {
            let result = app.call_method1(py, "handle_read_input_registers", (address, count));
            match result.and_then(|v| v.extract::<Vec<u16>>(py)) {
                Ok(regs) if regs.len() == count as usize => {
                    ModbusResponse::registers(FunctionCode::ReadInputRegisters, &regs)
                }
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadInputRegisters, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadInputRegisters, e),
            }
        }

        // FC06 — Write Single Register
        #[cfg(feature = "registers")]
        ModbusRequest::WriteSingleRegister { address, value, .. } => {
            let result = app.call_method1(py, "handle_write_register", (address, value));
            match result {
                Ok(_) => ModbusResponse::echo_register(address, value),
                Err(e) => py_error_to_response(py, FunctionCode::WriteSingleRegister, e),
            }
        }

        // FC10 — Write Multiple Registers
        #[cfg(feature = "registers")]
        ModbusRequest::WriteMultipleRegisters { address, count, data, .. } => {
            let result = app.call_method1(py, "handle_write_registers", (address, count, data.as_slice()));
            match result {
                Ok(_) => ModbusResponse::echo_multi_write(FunctionCode::WriteMultipleRegisters, address, count),
                Err(e) => py_error_to_response(py, FunctionCode::WriteMultipleRegisters, e),
            }
        }

        // FC16 — Mask Write Register
        #[cfg(feature = "registers")]
        ModbusRequest::MaskWriteRegister { address, and_mask, or_mask, .. } => {
            let result = app.call_method1(py, "handle_mask_write_register", (address, and_mask, or_mask));
            match result {
                Ok(_) => ModbusResponse::echo_mask_write(address, and_mask, or_mask),
                Err(e) => py_error_to_response(py, FunctionCode::MaskWriteRegister, e),
            }
        }

        // FC17 — Read/Write Multiple Registers
        #[cfg(feature = "registers")]
        ModbusRequest::ReadWriteMultipleRegisters {
            read_address, read_count, write_address, write_count, data, ..
        } => {
            let result = app.call_method1(
                py,
                "handle_read_write_registers",
                (read_address, read_count, write_address, write_count, data.as_slice()),
            );
            match result.and_then(|v| v.extract::<Vec<u16>>(py)) {
                Ok(regs) if regs.len() == read_count as usize => {
                    ModbusResponse::registers(FunctionCode::ReadWriteMultipleRegisters, &regs)
                }
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadWriteMultipleRegisters, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadWriteMultipleRegisters, e),
            }
        }

        // FC18 — Read FIFO Queue
        #[cfg(feature = "fifo")]
        ModbusRequest::ReadFifoQueue { pointer_address, .. } => {
            let result = app.call_method1(py, "handle_read_fifo_queue", (pointer_address,));
            match result.and_then(|v| v.extract::<Vec<u16>>(py)) {
                Ok(values) if values.len() <= 31 => {
                    // Payload: fifo_count (2 BE) + values (2 BE each)
                    let fifo_count = values.len() as u16;
                    let mut payload = heapless::Vec::<u8, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::new();
                    let _ = payload.extend_from_slice(&fifo_count.to_be_bytes());
                    for v in &values {
                        let _ = payload.extend_from_slice(&v.to_be_bytes());
                    }
                    ModbusResponse::fifo_response(&payload)
                }
                Ok(_) => ModbusResponse::exception(FunctionCode::ReadFifoQueue, ExceptionCode::IllegalDataAddress),
                Err(e) => py_error_to_response(py, FunctionCode::ReadFifoQueue, e),
            }
        }

        // FC07 — Read Exception Status
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReadExceptionStatus { .. } => {
            match app.call_method0(py, "handle_read_exception_status")
                .and_then(|v| v.extract::<u8>(py))
            {
                Ok(status) => ModbusResponse::read_exception_status(status),
                Err(e) => py_error_to_response(py, FunctionCode::ReadExceptionStatus, e),
            }
        }

        // FC08 — Diagnostics
        #[cfg(feature = "diagnostics")]
        ModbusRequest::Diagnostics { sub_function, data, .. } => {
            let result = app.call_method1(py, "handle_diagnostics", (sub_function, data));
            match result.and_then(|v| v.extract::<(u16, u16)>(py)) {
                Ok((response_sub_fn, response_data)) => {
                    ModbusResponse::diagnostics_echo(response_sub_fn, response_data)
                }
                Err(e) if e.is_instance_of::<PyNotImplementedError>(py) => {
                    ModbusResponse::diagnostics_echo(sub_function, data)
                }
                Err(e) => py_error_to_response(py, FunctionCode::Diagnostics, e),
            }
        }

        // FC0B — Get Comm Event Counter
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventCounter { .. } => {
            match app.call_method0(py, "handle_get_comm_event_counter")
                .and_then(|v| v.extract::<(u16, u16)>(py))
            {
                Ok((status, count)) => ModbusResponse::comm_event_counter(status, count),
                Err(e) => py_error_to_response(py, FunctionCode::GetCommEventCounter, e),
            }
        }

        // FC0C — Get Comm Event Log
        #[cfg(feature = "diagnostics")]
        ModbusRequest::GetCommEventLog { .. } => {
            match app
                .call_method0(py, "handle_get_comm_event_log")
                .and_then(|v| v.extract::<Vec<u8>>(py))
            {
                Ok(payload) => ModbusResponse::comm_event_log(&payload),
                Err(e) => py_error_to_response(py, FunctionCode::GetCommEventLog, e),
            }
        }

        // FC11 — Report Server ID
        #[cfg(feature = "diagnostics")]
        ModbusRequest::ReportServerId { .. } => {
            match app
                .call_method0(py, "handle_report_server_id")
                .and_then(|v| v.extract::<Vec<u8>>(py))
            {
                Ok(payload) => ModbusResponse::report_server_id(&payload),
                Err(e) => py_error_to_response(py, FunctionCode::ReportServerId, e),
            }
        }

        // All other variants → IllegalFunction
        other => {
            let fc_byte = other.function_code_byte();
            invalid_function_for_fc_byte(fc_byte)
        }
    }
}
