use std::sync::OnceLock;

use mbus_core::models::coil::Coils;
use mbus_core::models::discrete_input::DiscreteInputs;
use mbus_core::models::fifo_queue::FifoQueue;
use mbus_core::models::register::Registers;
use pyo3::prelude::*;
use tokio::runtime::Runtime;

// ── Shared Tokio runtime ─────────────────────────────────────────────────────

static TOKIO_RT: OnceLock<Runtime> = OnceLock::new();

/// Returns a reference to the module-wide Tokio runtime.
///
/// Initialised on first call; the same runtime is shared by all sync clients
/// so we never spin up redundant OS threads.
pub fn get_runtime() -> &'static Runtime {
    TOKIO_RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to create Tokio runtime for modbus_rs")
    })
}

/// Enter the shared Tokio runtime context.
///
/// Some constructors need an active runtime handle to spawn background tasks.
pub fn enter_runtime() -> tokio::runtime::EnterGuard<'static> {
    get_runtime().enter()
}

// ── Response converters ──────────────────────────────────────────────────────

/// Convert a `Coils` value into a Python `list[bool]`.
pub fn coils_to_py(py: Python<'_>, coils: Coils) -> PyResult<Py<PyAny>> {
    let qty = coils.quantity(); // already u16
    let base = coils.from_address();
    let list: Vec<bool> = (0..qty)
        .map(|i| coils.value(base + i).unwrap_or(false))
        .collect();
    Ok(list.into_pyobject(py)?.into_any().unbind())
}

/// Convert a `DiscreteInputs` value into a Python `list[bool]`.
pub fn discrete_inputs_to_py(py: Python<'_>, di: DiscreteInputs) -> PyResult<Py<PyAny>> {
    let qty = di.quantity(); // already u16
    let base = di.from_address();
    let list: Vec<bool> = (0..qty)
        .map(|i| di.value(base + i).unwrap_or(false))
        .collect();
    Ok(list.into_pyobject(py)?.into_any().unbind())
}

/// Convert a `Registers` value into a Python `list[int]`.
pub fn registers_to_py(py: Python<'_>, regs: Registers) -> PyResult<Py<PyAny>> {
    let qty = regs.quantity(); // already u16
    let base = regs.from_address();
    let list: Vec<u16> = (0..qty)
        .map(|i| regs.value(base + i).unwrap_or(0))
        .collect();
    Ok(list.into_pyobject(py)?.into_any().unbind())
}

/// Convert a `FifoQueue` value into a Python `list[int]`.
pub fn fifo_to_py(py: Python<'_>, queue: FifoQueue) -> PyResult<Py<PyAny>> {
    let list: Vec<u16> = queue.queue().iter().copied().collect();
    Ok(list.into_pyobject(py)?.into_any().unbind())
}

/// Convert `AsyncError` to a `PyErr` using our exception hierarchy.
pub use crate::python::errors::async_error_to_py;
