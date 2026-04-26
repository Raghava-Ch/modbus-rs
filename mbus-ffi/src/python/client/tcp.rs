use std::sync::Arc;
use std::time::Duration;

use mbus_client_async::AsyncTcpClient as InnerAsyncTcpClient;
#[cfg(feature = "diagnostics")]
use mbus_client_async::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "file-record")]
use mbus_client_async::{SubRequest, SubRequestParams};
use mbus_core::models::coil::Coils;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes::tokio::future_into_py;

use super::helpers::{
    async_error_to_py, coils_to_py, discrete_inputs_to_py, fifo_to_py, registers_to_py,
    enter_runtime, get_runtime,
};

// ── shared constructor helper ────────────────────────────────────────────────

fn make_inner(host: &str, port: u16, timeout_ms: u64) -> PyResult<InnerAsyncTcpClient> {
    // AsyncTcpClient::new() calls Handle::try_current() internally to spawn
    // its background task; we must be inside the runtime context when it runs.
    let _guard = enter_runtime();
    let client = InnerAsyncTcpClient::new(host, port).map_err(async_error_to_py)?;
    if timeout_ms > 0 {
        client.set_request_timeout(Duration::from_millis(timeout_ms));
    }
    Ok(client)
}

#[cfg(feature = "diagnostics")]
fn parse_device_id_kind(kind: &str) -> PyResult<ReadDeviceIdCode> {
    match kind.to_lowercase().as_str() {
        "basic" => Ok(ReadDeviceIdCode::Basic),
        "regular" => Ok(ReadDeviceIdCode::Regular),
        "extended" => Ok(ReadDeviceIdCode::Extended),
        "specific" => Ok(ReadDeviceIdCode::Specific),
        other => Err(crate::python::errors::ModbusInvalidArgument::new_err(format!(
            "Unknown device-identification kind '{other}'; expected 'basic', 'regular', 'extended', or 'specific'"
        ))),
    }
}

#[cfg(feature = "file-record")]
fn build_read_file_sub_request(requests: &[(u16, u16, u16)]) -> PyResult<SubRequest> {
    let mut sub_request = SubRequest::new();
    for (file_number, record_number, record_length) in requests {
        sub_request
            .add_read_sub_request(*file_number, *record_number, *record_length)
            .map_err(crate::python::errors::mbus_error_to_py)?;
    }
    Ok(sub_request)
}

#[cfg(feature = "file-record")]
fn build_write_file_sub_request(requests: &[(u16, u16, Vec<u16>)]) -> PyResult<SubRequest> {
    let mut sub_request = SubRequest::new();
    for (file_number, record_number, data) in requests {
        let record_data =
            heapless::Vec::<u16, { mbus_core::data_unit::common::MAX_PDU_DATA_LEN }>::from_slice(
                data.as_slice(),
            )
            .map_err(|_| crate::python::errors::ModbusInvalidArgument::new_err(
                "record_data exceeds Modbus PDU capacity",
            ))?;
        let record_length = record_data.len() as u16;
        sub_request
            .add_write_sub_request(*file_number, *record_number, record_length, record_data)
            .map_err(crate::python::errors::mbus_error_to_py)?;
    }
    Ok(sub_request)
}

#[cfg(feature = "file-record")]
fn file_record_read_to_py(py: Python<'_>, rows: Vec<SubRequestParams>) -> PyResult<Py<PyAny>> {
    let mut out: Vec<Py<PyAny>> = Vec::with_capacity(rows.len());
    for row in rows {
        let dict = PyDict::new(py);
        dict.set_item("file_number", row.file_number)?;
        dict.set_item("record_number", row.record_number)?;
        dict.set_item("record_length", row.record_length)?;
        match row.record_data {
            Some(values) => {
                let py_values: Vec<u16> = values.iter().copied().collect();
                dict.set_item("record_data", py_values)?;
            }
            None => {
                dict.set_item("record_data", py.None())?;
            }
        }
        out.push(dict.into_any().unbind());
    }
    Ok(out.into_pyobject(py)?.into_any().unbind())
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync TCP client
// ═══════════════════════════════════════════════════════════════════════════

/// Synchronous (blocking) Modbus TCP client.
///
/// Each method blocks the calling thread until a response arrives, then
/// returns the value directly. The GIL is released during network I/O so
/// other Python threads are not starved.
///
/// Use as a context manager:
///
/// ```python
/// with TcpClient("192.168.1.10", unit_id=1) as client:
///     regs = client.read_holding_registers(0, 5)
/// ```
#[pyclass(name = "TcpClient")]
pub struct TcpClient {
    inner: InnerAsyncTcpClient,
    unit_id: u8,
}

#[pymethods]
impl TcpClient {
    /// Create a new sync TCP client.
    ///
    /// :param host: Hostname or IP address of the Modbus TCP server.
    /// :param port: TCP port (default 502).
    /// :param unit_id: Modbus unit / slave ID (1–247, default 1).
    /// :param timeout_ms: Per-request timeout in milliseconds (0 = disabled, default 1000).
    #[new]
    #[pyo3(signature = (host, port=502, unit_id=1, timeout_ms=1000))]
    fn new(host: &str, port: u16, unit_id: u8, timeout_ms: u64) -> PyResult<Self> {
        let inner = make_inner(host, port, timeout_ms)?;
        Ok(Self { inner, unit_id })
    }

    /// Establish the TCP connection. Must be called before any request.
    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        py.detach(|| rt.block_on(self.inner.connect()).map_err(async_error_to_py))
    }

    /// Disconnect from the server.
    fn disconnect(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        py.detach(|| rt.block_on(self.inner.disconnect()).map_err(async_error_to_py))
    }

    /// Returns ``True`` if there are requests currently in-flight.
    fn has_pending_requests(&self) -> bool {
        self.inner.has_pending_requests()
    }

    fn __enter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __exit__(
        &self,
        py: Python<'_>,
        _exc_type: Option<Bound<'_, PyAny>>,
        _exc_val: Option<Bound<'_, PyAny>>,
        _exc_tb: Option<Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        self.disconnect(py)?;
        Ok(false)
    }

    // ── Coils ────────────────────────────────────────────────────────────

    /// Read coils (FC 01). Returns ``list[bool]``.
    #[pyo3(signature = (address, quantity))]
    fn read_coils(&self, py: Python<'_>, address: u16, quantity: u16) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_multiple_coils(uid, address, quantity))
                .map_err(async_error_to_py)
        })?;
        coils_to_py(py, result)
    }

    /// Write a single coil (FC 05). Returns ``(address, value)`` echo.
    #[pyo3(signature = (address, value))]
    fn write_coil(&self, py: Python<'_>, address: u16, value: bool) -> PyResult<(u16, bool)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.write_single_coil(uid, address, value))
                .map_err(async_error_to_py)
        })
    }

    /// Write multiple coils (FC 0F). Returns ``(start_address, quantity)`` echo.
    #[pyo3(signature = (address, values))]
    fn write_coils(
        &self,
        py: Python<'_>,
        address: u16,
        values: Vec<bool>,
    ) -> PyResult<(u16, u16)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let qty = values.len() as u16;
        let mut coils =
            Coils::new(address, qty).map_err(crate::python::errors::mbus_error_to_py)?;
        for (i, &v) in values.iter().enumerate() {
            coils
                .set_value(address + i as u16, v)
                .map_err(crate::python::errors::mbus_error_to_py)?;
        }
        py.detach(|| {
            rt.block_on(self.inner.write_multiple_coils(uid, address, &coils))
                .map_err(async_error_to_py)
        })
    }

    // ── Discrete inputs ──────────────────────────────────────────────────

    /// Read discrete inputs (FC 02). Returns ``list[bool]``.
    #[pyo3(signature = (address, quantity))]
    fn read_discrete_inputs(
        &self,
        py: Python<'_>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_discrete_inputs(uid, address, quantity))
                .map_err(async_error_to_py)
        })?;
        discrete_inputs_to_py(py, result)
    }

    // ── Registers ────────────────────────────────────────────────────────

    /// Read holding registers (FC 03). Returns ``list[int]``.
    #[pyo3(signature = (address, quantity))]
    fn read_holding_registers(
        &self,
        py: Python<'_>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_holding_registers(uid, address, quantity))
                .map_err(async_error_to_py)
        })?;
        registers_to_py(py, result)
    }

    /// Read input registers (FC 04). Returns ``list[int]``.
    #[pyo3(signature = (address, quantity))]
    fn read_input_registers(
        &self,
        py: Python<'_>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_input_registers(uid, address, quantity))
                .map_err(async_error_to_py)
        })?;
        registers_to_py(py, result)
    }

    /// Write a single holding register (FC 06). Returns ``(address, value)`` echo.
    #[pyo3(signature = (address, value))]
    fn write_register(&self, py: Python<'_>, address: u16, value: u16) -> PyResult<(u16, u16)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.write_single_register(uid, address, value))
                .map_err(async_error_to_py)
        })
    }

    /// Write multiple holding registers (FC 10). Returns ``(start_address, quantity)`` echo.
    #[pyo3(signature = (address, values))]
    fn write_registers(
        &self,
        py: Python<'_>,
        address: u16,
        values: Vec<u16>,
    ) -> PyResult<(u16, u16)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(
                self.inner
                    .write_multiple_registers(uid, address, values.as_slice()),
            )
            .map_err(async_error_to_py)
        })
    }

    /// Mask-write a holding register (FC 16).
    #[pyo3(signature = (address, and_mask, or_mask))]
    fn mask_write_register(
        &self,
        py: Python<'_>,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> PyResult<()> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(
                self.inner
                    .mask_write_register(uid, address, and_mask, or_mask),
            )
            .map_err(async_error_to_py)
        })
    }

    /// Read/write multiple registers (FC 17). Returns ``list[int]`` of read registers.
    #[pyo3(signature = (read_address, read_quantity, write_address, write_values))]
    fn read_write_registers(
        &self,
        py: Python<'_>,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: Vec<u16>,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_write_multiple_registers(
                uid,
                read_address,
                read_quantity,
                write_address,
                write_values.as_slice(),
            ))
            .map_err(async_error_to_py)
        })?;
        registers_to_py(py, result)
    }

    // ── FIFO ─────────────────────────────────────────────────────────────

    /// Read FIFO queue (FC 18). Returns ``list[int]``.
    #[pyo3(signature = (address))]
    fn read_fifo_queue(&self, py: Python<'_>, address: u16) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let result = py.detach(|| {
            rt.block_on(self.inner.read_fifo_queue(uid, address))
                .map_err(async_error_to_py)
        })?;
        fifo_to_py(py, result)
    }

    // ── File record ──────────────────────────────────────────────────────

    /// Read file record (FC 14).
    ///
    /// :param requests: ``list[tuple[file_number, record_number, record_length]]``
    /// :returns: ``list[dict]`` with keys: ``file_number``, ``record_number``,
    ///           ``record_length``, ``record_data``.
    #[pyo3(signature = (requests))]
    #[cfg(feature = "file-record")]
    fn read_file_record(
        &self,
        py: Python<'_>,
        requests: Vec<(u16, u16, u16)>,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let sub_request = build_read_file_sub_request(&requests)?;
        let rows = py.detach(|| {
            rt.block_on(self.inner.read_file_record(uid, &sub_request))
                .map_err(async_error_to_py)
        })?;
        file_record_read_to_py(py, rows)
    }

    /// Write file record (FC 15).
    ///
    /// :param requests: ``list[tuple[file_number, record_number, data_words]]``
    #[pyo3(signature = (requests))]
    #[cfg(feature = "file-record")]
    fn write_file_record(
        &self,
        py: Python<'_>,
        requests: Vec<(u16, u16, Vec<u16>)>,
    ) -> PyResult<()> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let sub_request = build_write_file_sub_request(&requests)?;
        py.detach(|| {
            rt.block_on(self.inner.write_file_record(uid, &sub_request))
                .map_err(async_error_to_py)
        })
    }

    // ── Diagnostics ──────────────────────────────────────────────────────

    /// Read exception status (FC 07). Returns ``int`` status byte.
    fn read_exception_status(&self, py: Python<'_>) -> PyResult<u8> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.read_exception_status(uid))
                .map_err(async_error_to_py)
        })
    }

    /// Read comm event counter (FC 0B). Returns ``(status_word, event_count)``.
    fn get_event_counter(&self, py: Python<'_>) -> PyResult<(u16, u16)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.get_comm_event_counter(uid))
                .map_err(async_error_to_py)
        })
    }

    /// Diagnostics (FC 08).
    ///
    /// :param sub_function: Diagnostic sub-function code (u16).
    /// :param data: Data words echoed/processed per sub-function.
    /// :returns: ``(sub_function, data_words)``
    #[pyo3(signature = (sub_function, data))]
    #[cfg(feature = "diagnostics")]
    fn diagnostics(
        &self,
        py: Python<'_>,
        sub_function: u16,
        data: Vec<u16>,
    ) -> PyResult<(u16, Vec<u16>)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let sub_fn =
            DiagnosticSubFunction::try_from(sub_function).map_err(crate::python::errors::mbus_error_to_py)?;
        let response = py.detach(|| {
            rt.block_on(self.inner.diagnostics(uid, sub_fn, data.as_slice()))
                .map_err(async_error_to_py)
        })?;
        Ok((u16::from(response.sub_function), response.data))
    }

    /// Get communication event log (FC 0C).
    #[cfg(feature = "diagnostics")]
    fn get_comm_event_log(&self, py: Python<'_>) -> PyResult<(u16, u16, u16, Vec<u8>)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.get_comm_event_log(uid))
                .map_err(async_error_to_py)
        })
    }

    /// Report server ID (FC 11).
    #[cfg(feature = "diagnostics")]
    fn report_server_id(&self, py: Python<'_>) -> PyResult<Vec<u8>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.report_server_id(uid))
                .map_err(async_error_to_py)
        })
    }

    /// Read device identification (FC 2B / MEI 14).
    ///
    /// Returns a ``dict[int, bytes]`` mapping object ID to its value.
    #[pyo3(signature = (object_id=0, kind="basic"))]
    fn get_device_identification(
        &self,
        py: Python<'_>,
        object_id: u8,
        kind: &str,
    ) -> PyResult<Py<PyAny>> {
        let rt = get_runtime();
        let uid = self.unit_id;
        let oid = ObjectId::from(object_id);
        let read_code = parse_device_id_kind(kind)?;
        let result = py.detach(|| {
            rt.block_on(
                self.inner
                    .read_device_identification(uid, read_code, oid),
            )
            .map_err(async_error_to_py)
        })?;
        let dict = PyDict::new(py);
        for obj in result.objects().flatten() {
            dict.set_item(u8::from(obj.object_id), obj.value.as_slice())?;
        }
        Ok(dict.into_any().unbind())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Async TCP client
// ═══════════════════════════════════════════════════════════════════════════

/// Asyncio Modbus TCP client.
///
/// All methods return awaitables; use with ``await`` or ``asyncio.gather``:
///
/// ```python
/// async with AsyncTcpClient("192.168.1.10", unit_id=1) as client:
///     regs = await client.read_holding_registers(0, 5)
/// ```
#[pyclass(name = "AsyncTcpClient")]
pub struct AsyncTcpClient {
    inner: Arc<InnerAsyncTcpClient>,
    unit_id: u8,
}

#[pymethods]
impl AsyncTcpClient {
    #[new]
    #[pyo3(signature = (host, port=502, unit_id=1, timeout_ms=1000))]
    fn new(host: &str, port: u16, unit_id: u8, timeout_ms: u64) -> PyResult<Self> {
        let inner = Arc::new(make_inner(host, port, timeout_ms)?);
        Ok(Self { inner, unit_id })
    }

    fn connect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        future_into_py(py, async move {
            client.connect().await.map_err(async_error_to_py)
        })
    }

    fn has_pending_requests(&self) -> bool {
        self.inner.has_pending_requests()
    }

    fn __aenter__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = slf.inner.clone();
        let this = slf.into_pyobject(py)?.into_any().unbind();
        future_into_py(py, async move {
            client.connect().await.map_err(async_error_to_py)?;
            Ok::<Py<PyAny>, PyErr>(this)
        })
    }

    fn disconnect<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        future_into_py(py, async move {
            client.disconnect().await.map_err(async_error_to_py)
        })
    }

    fn __aexit__<'py>(
        &self,
        py: Python<'py>,
        _exc_type: Option<Bound<'py, PyAny>>,
        _exc_val: Option<Bound<'py, PyAny>>,
        _exc_tb: Option<Bound<'py, PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        future_into_py(py, async move {
            let _ = client.disconnect().await;
            Ok::<bool, PyErr>(false)
        })
    }

    // ── Coils ────────────────────────────────────────────────────────────

    #[pyo3(signature = (address, quantity))]
    fn read_coils<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let coils = client
                .read_multiple_coils(uid, address, quantity)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| coils_to_py(py, coils))
        })
    }

    #[pyo3(signature = (address, value))]
    fn write_coil<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        value: bool,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .write_single_coil(uid, address, value)
                .await
                .map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (address, values))]
    fn write_coils<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        values: Vec<bool>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        let qty = values.len() as u16;
        let mut coils =
            Coils::new(address, qty).map_err(crate::python::errors::mbus_error_to_py)?;
        for (i, &v) in values.iter().enumerate() {
            coils
                .set_value(address + i as u16, v)
                .map_err(crate::python::errors::mbus_error_to_py)?;
        }
        future_into_py(py, async move {
            client
                .write_multiple_coils(uid, address, &coils)
                .await
                .map_err(async_error_to_py)
        })
    }

    // ── Discrete inputs ──────────────────────────────────────────────────

    #[pyo3(signature = (address, quantity))]
    fn read_discrete_inputs<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let di = client
                .read_discrete_inputs(uid, address, quantity)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| discrete_inputs_to_py(py, di))
        })
    }

    // ── Registers ────────────────────────────────────────────────────────

    #[pyo3(signature = (address, quantity))]
    fn read_holding_registers<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let regs = client
                .read_holding_registers(uid, address, quantity)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| registers_to_py(py, regs))
        })
    }

    #[pyo3(signature = (address, quantity))]
    fn read_input_registers<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        quantity: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let regs = client
                .read_input_registers(uid, address, quantity)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| registers_to_py(py, regs))
        })
    }

    #[pyo3(signature = (address, value))]
    fn write_register<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        value: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .write_single_register(uid, address, value)
                .await
                .map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (address, values))]
    fn write_registers<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        values: Vec<u16>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .write_multiple_registers(uid, address, values.as_slice())
                .await
                .map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (address, and_mask, or_mask))]
    fn mask_write_register<'py>(
        &self,
        py: Python<'py>,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .mask_write_register(uid, address, and_mask, or_mask)
                .await
                .map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (read_address, read_quantity, write_address, write_values))]
    fn read_write_registers<'py>(
        &self,
        py: Python<'py>,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: Vec<u16>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let regs = client
                .read_write_multiple_registers(
                    uid,
                    read_address,
                    read_quantity,
                    write_address,
                    write_values.as_slice(),
                )
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| registers_to_py(py, regs))
        })
    }

    // ── FIFO ─────────────────────────────────────────────────────────────

    #[pyo3(signature = (address))]
    fn read_fifo_queue<'py>(&self, py: Python<'py>, address: u16) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            let queue = client
                .read_fifo_queue(uid, address)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| fifo_to_py(py, queue))
        })
    }

    // ── File record ──────────────────────────────────────────────────────

    #[pyo3(signature = (requests))]
    #[cfg(feature = "file-record")]
    fn read_file_record<'py>(
        &self,
        py: Python<'py>,
        requests: Vec<(u16, u16, u16)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        let sub_request = build_read_file_sub_request(&requests)?;
        future_into_py(py, async move {
            let rows = client
                .read_file_record(uid, &sub_request)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| file_record_read_to_py(py, rows))
        })
    }

    #[pyo3(signature = (requests))]
    #[cfg(feature = "file-record")]
    fn write_file_record<'py>(
        &self,
        py: Python<'py>,
        requests: Vec<(u16, u16, Vec<u16>)>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        let sub_request = build_write_file_sub_request(&requests)?;
        future_into_py(py, async move {
            client
                .write_file_record(uid, &sub_request)
                .await
                .map_err(async_error_to_py)
        })
    }

    // ── Diagnostics ──────────────────────────────────────────────────────

    fn read_exception_status<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .read_exception_status(uid)
                .await
                .map_err(async_error_to_py)
        })
    }

    fn get_event_counter<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .get_comm_event_counter(uid)
                .await
                .map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (sub_function, data))]
    #[cfg(feature = "diagnostics")]
    fn diagnostics<'py>(
        &self,
        py: Python<'py>,
        sub_function: u16,
        data: Vec<u16>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        let sub_fn =
            DiagnosticSubFunction::try_from(sub_function).map_err(crate::python::errors::mbus_error_to_py)?;
        future_into_py(py, async move {
            let response = client
                .diagnostics(uid, sub_fn, data.as_slice())
                .await
                .map_err(async_error_to_py)?;
            Ok::<(u16, Vec<u16>), PyErr>((u16::from(response.sub_function), response.data))
        })
    }

    #[cfg(feature = "diagnostics")]
    fn get_comm_event_log<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client
                .get_comm_event_log(uid)
                .await
                .map_err(async_error_to_py)
        })
    }

    #[cfg(feature = "diagnostics")]
    fn report_server_id<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        future_into_py(py, async move {
            client.report_server_id(uid).await.map_err(async_error_to_py)
        })
    }

    #[pyo3(signature = (object_id=0, kind="basic"))]
    fn get_device_identification<'py>(
        &self,
        py: Python<'py>,
        object_id: u8,
        kind: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let client = self.inner.clone();
        let uid = self.unit_id;
        let oid = ObjectId::from(object_id);
        let read_code = parse_device_id_kind(kind)?;
        future_into_py(py, async move {
            let result = client
                .read_device_identification(uid, read_code, oid)
                .await
                .map_err(async_error_to_py)?;
            Python::attach(|py| {
                let dict = PyDict::new(py);
                for obj in result.objects().flatten() {
                    dict.set_item(u8::from(obj.object_id), obj.value.as_slice())?;
                }
                Ok::<Py<PyAny>, PyErr>(dict.into_any().unbind())
            })
        })
    }
}
