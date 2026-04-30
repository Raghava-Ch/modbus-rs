use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use mbus_client_async::AsyncSerialClient as InnerAsyncSerialClient;
#[cfg(feature = "diagnostics")]
use mbus_client_async::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "file-record")]
use mbus_client_async::{SubRequest, SubRequestParams};
use mbus_core::models::coil::Coils;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use pyo3_async_runtimes::tokio::future_into_py;

use super::helpers::{
    async_error_to_py, coils_to_py, discrete_inputs_to_py, enter_runtime, fifo_to_py, get_runtime,
    registers_to_py,
};

// ── shared constructor helper ────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn make_serial_config(
    port: &str,
    baud_rate: u32,
    timeout_ms: u64,
    mode: SerialMode,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: u8,
    retry_attempts: u8,
) -> PyResult<ModbusSerialConfig> {
    let port_path = heapless::String::<64>::from_str(port)
        .map_err(|_| crate::python::errors::ModbusConfigError::new_err(
            format!("Port path too long (max 64 chars): {port}"),
        ))?;
    Ok(ModbusSerialConfig {
        port_path,
        mode,
        baud_rate: normalize_baud_rate(baud_rate),
        data_bits,
        stop_bits,
        parity,
        response_timeout_ms: timeout_ms as u32,
        retry_attempts,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    })
}

fn normalize_baud_rate(baud_rate: u32) -> BaudRate {
    match baud_rate {
        9600 => BaudRate::Baud9600,
        19200 => BaudRate::Baud19200,
        other => BaudRate::Custom(other),
    }
}

fn parse_data_bits(data_bits: u8) -> PyResult<DataBits> {
    match data_bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Invalid data_bits '{other}'; expected one of 5, 6, 7, or 8"
        ))),
    }
}

fn parse_parity(parity: &str) -> PyResult<Parity> {
    match parity.to_lowercase().as_str() {
        "none" | "n" => Ok(Parity::None),
        "even" | "e" => Ok(Parity::Even),
        "odd" | "o" => Ok(Parity::Odd),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Unknown parity '{other}'; expected 'none', 'even', or 'odd'"
        ))),
    }
}

fn parse_stop_bits(stop_bits: u8) -> PyResult<u8> {
    match stop_bits {
        1 | 2 => Ok(stop_bits),
        other => Err(crate::python::errors::ModbusConfigError::new_err(format!(
            "Invalid stop_bits '{other}'; expected 1 or 2"
        ))),
    }
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

#[allow(clippy::too_many_arguments)]
fn make_inner(
    port: &str,
    baud_rate: u32,
    timeout_ms: u64,
    mode: SerialMode,
    data_bits: DataBits,
    parity: Parity,
    stop_bits: u8,
    retry_attempts: u8,
) -> PyResult<InnerAsyncSerialClient> {
    // AsyncSerialClient::new_*() calls Handle::try_current() internally to
    // spawn its background task; we must be inside the runtime context.
    let _guard = enter_runtime();
    let cfg = make_serial_config(
        port,
        baud_rate,
        timeout_ms,
        mode,
        data_bits,
        parity,
        stop_bits,
        retry_attempts,
    )?;
    let client = match mode {
        SerialMode::Rtu => {
            InnerAsyncSerialClient::new_rtu(cfg).map_err(async_error_to_py)?
        }
        SerialMode::Ascii => {
            InnerAsyncSerialClient::new_ascii(cfg).map_err(async_error_to_py)?
        }
    };
    if timeout_ms > 0 {
        client.set_request_timeout(Duration::from_millis(timeout_ms));
    }
    Ok(client)
}

// ═══════════════════════════════════════════════════════════════════════════
// Sync Serial client
// ═══════════════════════════════════════════════════════════════════════════

/// Synchronous (blocking) Modbus serial client (RTU or ASCII).
///
/// ```python
/// with SerialClient("/dev/ttyUSB0", baud_rate=9600, unit_id=1) as client:
///     regs = client.read_holding_registers(0, 5)
/// ```
#[pyclass(name = "SerialClient")]
pub struct SerialClient {
    inner: InnerAsyncSerialClient,
    unit_id: u8,
}

#[pymethods]
impl SerialClient {
    /// Create a new sync serial client.
    ///
    /// :param port: Serial port path (e.g. ``"/dev/ttyUSB0"`` or ``"COM3"``).
    /// :param baud_rate: Baud rate in bits/s (default 9600).
    /// :param unit_id: Modbus unit / slave ID (1–247, default 1).
    /// :param mode: Framing mode as ``"rtu"``/``"ascii"`` or ``SerialMode.RTU``/``SerialMode.ASCII``.
    /// :param timeout_ms: Per-request timeout in milliseconds (default 1000).
    /// :param data_bits: Data bits per serial character (5/6/7/8, default 8).
    /// :param parity: Parity mode: ``"none"`` (default), ``"even"``, or ``"odd"``.
    /// :param stop_bits: Stop bit count (1 or 2, default 1).
    /// :param retry_attempts: Number of client retry attempts (default 3).
    #[new]
    #[pyo3(signature = (port, baud_rate=9600, unit_id=1, mode=None, timeout_ms=1000, data_bits=8, parity="none", stop_bits=1, retry_attempts=3))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        port: &str,
        baud_rate: u32,
        unit_id: u8,
        mode: Option<Py<PyAny>>,
        timeout_ms: u64,
        data_bits: u8,
        parity: &str,
        stop_bits: u8,
        retry_attempts: u8,
    ) -> PyResult<Self> {
        let serial_mode = match mode {
            Some(mode) => crate::python::parse_serial_mode_any(mode.bind(py))?,
            None => SerialMode::Rtu,
        };
        let data_bits = parse_data_bits(data_bits)?;
        let parity = parse_parity(parity)?;
        let stop_bits = parse_stop_bits(stop_bits)?;
        let inner = make_inner(
            port,
            baud_rate,
            timeout_ms,
            serial_mode,
            data_bits,
            parity,
            stop_bits,
            retry_attempts,
        )?;
        Ok(Self { inner, unit_id })
    }

    fn connect(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        py.detach(|| {
            rt.block_on(self.inner.connect()).map_err(async_error_to_py)
        })
    }

    fn disconnect(&self, py: Python<'_>) -> PyResult<()> {
        let rt = get_runtime();
        py.detach(|| rt.block_on(self.inner.disconnect()).map_err(async_error_to_py))
    }

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

    #[pyo3(signature = (address, value))]
    fn write_coil(&self, py: Python<'_>, address: u16, value: bool) -> PyResult<(u16, bool)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.write_single_coil(uid, address, value))
                .map_err(async_error_to_py)
        })
    }

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

    #[pyo3(signature = (address, value))]
    fn write_register(&self, py: Python<'_>, address: u16, value: u16) -> PyResult<(u16, u16)> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.write_single_register(uid, address, value))
                .map_err(async_error_to_py)
        })
    }

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

    fn read_exception_status(&self, py: Python<'_>) -> PyResult<u8> {
        let rt = get_runtime();
        let uid = self.unit_id;
        py.detach(|| {
            rt.block_on(self.inner.read_exception_status(uid))
                .map_err(async_error_to_py)
        })
    }

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
// Async Serial client
// ═══════════════════════════════════════════════════════════════════════════

/// Asyncio Modbus serial client (RTU or ASCII).
///
/// ```python
/// async with AsyncSerialClient("/dev/ttyUSB0", baud_rate=9600, unit_id=1) as client:
///     regs = await client.read_holding_registers(0, 5)
/// ```
#[pyclass(name = "AsyncSerialClient")]
pub struct AsyncSerialClient {
    inner: Arc<InnerAsyncSerialClient>,
    unit_id: u8,
}

#[pymethods]
impl AsyncSerialClient {
    #[new]
    #[pyo3(signature = (port, baud_rate=9600, unit_id=1, mode=None, timeout_ms=1000, data_bits=8, parity="none", stop_bits=1, retry_attempts=3))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        py: Python<'_>,
        port: &str,
        baud_rate: u32,
        unit_id: u8,
        mode: Option<Py<PyAny>>,
        timeout_ms: u64,
        data_bits: u8,
        parity: &str,
        stop_bits: u8,
        retry_attempts: u8,
    ) -> PyResult<Self> {
        let serial_mode = match mode {
            Some(mode) => crate::python::parse_serial_mode_any(mode.bind(py))?,
            None => SerialMode::Rtu,
        };
        let data_bits = parse_data_bits(data_bits)?;
        let parity = parse_parity(parity)?;
        let stop_bits = parse_stop_bits(stop_bits)?;
        let inner = Arc::new(make_inner(
            port,
            baud_rate,
            timeout_ms,
            serial_mode,
            data_bits,
            parity,
            stop_bits,
            retry_attempts,
        )?);
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
