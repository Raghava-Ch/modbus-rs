use mbus_core::models::fifo_queue::FifoQueue;
#[cfg(feature = "fifo")]
use crate::c::MbusStatusCode;

// ── Opaque Handle ─────────────────────────────────────────────────────────────

/// Opaque handle to a FifoQueue instance (Rust-owned memory).
#[repr(C)]
pub struct MbusFifoQueue(pub(crate) FifoQueue);

impl MbusFifoQueue {
    #[cfg(feature = "fifo")]
    pub(in crate::c) fn inner(&self) -> &FifoQueue {
        &self.0
    }

    #[cfg(feature = "fifo")]
    pub(in crate::c) fn new(value: FifoQueue) -> Self {
        Self(value)
    }
}

// ── C API Functions ──────────────────────────────────────────────────────────

#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
/// Returns the FIFO pointer address.
pub unsafe extern "C" fn mbus_fifo_queue_ptr_address(fifo_queue: *const MbusFifoQueue) -> u16 {
    if fifo_queue.is_null() {
        return 0;
    }
    unsafe { (*fifo_queue).inner().ptr_address() }
}

#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
/// Returns the number of values in the FIFO queue.
pub unsafe extern "C" fn mbus_fifo_queue_count(fifo_queue: *const MbusFifoQueue) -> u16 {
    if fifo_queue.is_null() {
        return 0;
    }
    unsafe { (*fifo_queue).inner().length() as u16 }
}

#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
/// Reads a FIFO value by index.
pub unsafe extern "C" fn mbus_fifo_queue_value(
    fifo_queue: *const MbusFifoQueue,
    index: u16,
    out_value: *mut u16,
) -> MbusStatusCode {
    if fifo_queue.is_null() || out_value.is_null() {
        return MbusStatusCode::MbusErrNullPointer;
    }

    let q = unsafe { (*fifo_queue).inner() };
    let values = q.queue();
    let idx = index as usize;
    if idx >= values.len() {
        return MbusStatusCode::MbusErrInvalidOffset;
    }
    unsafe { *out_value = values[idx] };
    MbusStatusCode::MbusOk
}

#[cfg(feature = "fifo")]
#[unsafe(no_mangle)]
/// Returns a raw pointer to the FIFO values. Valid during callback only.
pub unsafe extern "C" fn mbus_fifo_queue_values_ptr(fifo_queue: *const MbusFifoQueue) -> *const u16 {
    if fifo_queue.is_null() {
        return core::ptr::null();
    }
    unsafe { (*fifo_queue).inner().queue().as_ptr() }
}
