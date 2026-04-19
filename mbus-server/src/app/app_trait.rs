//! Application Layer Traits
//!
//! This module defines the core traits used to bridge the Modbus protocol stack with
//! user-defined application logic. It follows a callback-based (observer) pattern
//! where the stack notifies the application of successful responses or failures.
//!
//! ## Trait Architecture
//!
//! Traits are split by functional group so that applications only implement what they need:
//!
//! | Trait | Feature | Function Codes |
//! |-------|---------|---------------|
//! | [`ServerExceptionHandler`] | always | exception notifications |
//! | [`ServerCoilHandler`] | `coils` | FC01, FC05, FC0F |
//! | [`ServerDiscreteInputHandler`] | `discrete-inputs` | FC02 |
//! | [`ServerHoldingRegisterHandler`] | `holding-registers` | FC03, FC06, FC10, FC16, FC17 |
//! | [`ServerInputRegisterHandler`] | `input-registers` | FC04 |
//! | [`ServerFifoHandler`] | `fifo` | FC18 |
//! | [`ServerFileRecordHandler`] | `file-record` | FC14, FC15 |
//! | [`ServerDiagnosticsHandler`] | `diagnostics` | FC07, FC08, FC0B, FC0C, FC11, FC2B |
//! | [`TrafficNotifier`] | `traffic` | TX/RX frame observability |
//!
//! [`ModbusAppHandler`] is a composed supertrait that auto-implements for any type
//! satisfying all enabled split traits. You never need to implement it directly.
//!
//! ## Callback Contract (applies to all traits in this file)
//!
//! - Callbacks are dispatched from `ServerServices::poll()`. No callback is invoked unless
//!   the application actively calls `poll()`.
//! - Callback implementations should remain lightweight and non-blocking. If heavy work is
//!   needed (database writes, UI updates, IPC), enqueue that work into your own task queue.
//! - `txn_id` is always the original id supplied by the caller, including Serial modes where
//!   transaction ids are not transmitted on the wire.

use mbus_core::{
    errors::{ExceptionCode, MbusError},
    function_codes::public::FunctionCode,
    transport::UnitIdOrSlaveAddr,
};

// #[cfg(feature = "coils")]
// use crate::Coils;
// #[cfg(feature = "diagnostics")]
// use crate::DeviceIdentificationResponse;
// #[cfg(feature = "discrete-inputs")]
// use crate::DiscreteInputs;
// #[cfg(feature = "fifo")]
// use crate::FifoQueue;
// #[cfg(feature = "file-record")]
// use crate::SubRequestParams;
#[cfg(feature = "traffic")]
/// Direction of raw Modbus frame traffic observed by the server stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficDirection {
    /// Outgoing response ADU sent by the server.
    Tx,
    /// Incoming request ADU received by the server.
    Rx,
}

#[cfg(feature = "traffic")]
/// Optional traffic notifications emitted by the server stack.
///
/// This trait is opt-in and enabled only with the `traffic` feature.  All
/// methods have default no-op implementations, so only override the ones you
/// care about.
pub trait TrafficNotifier {
    /// Called when a request frame has been received and is about to be dispatched.
    fn on_tx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
    }

    /// Called when an incoming request frame is accepted for dispatch.
    fn on_rx_frame(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _frame: &[u8],
    ) {
    }

    /// Called when sending a response frame failed.
    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }

    /// Called when processing an incoming request frame failed.
    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
        _frame: &[u8],
    ) {
    }
}

// ---------------------------------------------------------------------------
// Split trait definitions — each functional group is a standalone trait.
// ---------------------------------------------------------------------------

/// Handles exception notification callbacks from the server stack.
///
/// Override `on_exception` to log, count, or react to any exception the server sends.
/// The default implementation is a no-op.
pub trait ServerExceptionHandler {
    /// Called whenever the server sends a Modbus exception response to the master.
    fn on_exception(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _function_code: FunctionCode,
        _exception_code: ExceptionCode,
        _error: MbusError,
    ) {
    }
}

/// Handles coil-related requests (FC01, FC05, FC0F).
pub trait ServerCoilHandler {
    /// Handles a `Read Coils` (FC 0x01) request.
    fn read_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, quantity, out);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Write Single Coil` (FC 0x05) request.
    fn write_single_coil_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, value);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Write Multiple Coils` (FC 0x0F) request.
    fn write_multiple_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        let _ = (
            txn_id,
            unit_id_or_slave_addr,
            starting_address,
            quantity,
            values,
        );
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles discrete input requests (FC02).
pub trait ServerDiscreteInputHandler {
    /// Handles a `Read Discrete Inputs` (FC 0x02) request.
    fn read_discrete_inputs_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, quantity, out);
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles holding register requests (FC03, FC06, FC10, FC16, FC17).
pub trait ServerHoldingRegisterHandler {
    /// Handles a `Read Holding Registers` (FC 0x03) request.
    fn read_multiple_holding_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, quantity, out);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Write Single Register` (FC 0x06) request.
    fn write_single_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, value);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Write Multiple Registers` (FC 0x10) request.
    fn write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, starting_address, values);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Mask Write Register` (FC 0x16) request.
    fn mask_write_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, and_mask, or_mask);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Read/Write Multiple Registers` (FC 0x17) request.
    #[allow(clippy::too_many_arguments)]
    fn read_write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (
            txn_id,
            unit_id_or_slave_addr,
            read_address,
            read_quantity,
            write_address,
            write_values,
            out,
        );
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles input register requests (FC04).
pub trait ServerInputRegisterHandler {
    /// Handles a `Read Input Registers` (FC 0x04) request.
    fn read_multiple_input_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, quantity, out);
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles FIFO queue requests (FC18).
pub trait ServerFifoHandler {
    /// Handles a `Read FIFO Queue` (FC 0x18) request.
    fn read_fifo_queue_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, pointer_address, out);
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles file record requests (FC14, FC15).
pub trait ServerFileRecordHandler {
    /// Handles a `Read File Record` (FC 0x14) sub-request.
    fn read_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        let _ = (
            txn_id,
            unit_id_or_slave_addr,
            file_number,
            record_number,
            record_length,
            out,
        );
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Write File Record` (FC 0x15) sub-request.
    fn write_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: &[u16],
    ) -> Result<(), MbusError> {
        let _ = (
            txn_id,
            unit_id_or_slave_addr,
            file_number,
            record_number,
            record_length,
            record_data,
        );
        Err(MbusError::InvalidFunctionCode)
    }
}

/// Handles diagnostics-related requests (FC07, FC08, FC0B, FC0C, FC11, FC2B).
pub trait ServerDiagnosticsHandler {
    /// Handles a `Read Exception Status` (FC 0x07) request.
    fn read_exception_status_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Diagnostics` (FC 0x08) request.
    fn diagnostics_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: mbus_core::function_codes::public::DiagnosticSubFunction,
        data: u16,
    ) -> Result<u16, MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, sub_function, data);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Get Comm Event Counter` (FC 0x0B) request.
    fn get_comm_event_counter_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(u16, u16), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Get Comm Event Log` (FC 0x0C) request.
    fn get_comm_event_log_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_events: &mut [u8],
    ) -> Result<(u16, u16, u16, u8), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, out_events);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Report Server ID` (FC 0x11) request.
    fn report_server_id_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, out_server_id);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a `Read Device Identification` (FC 0x2B / MEI 0x0E) request.
    fn read_device_identification_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        let _ = (
            txn_id,
            unit_id_or_slave_addr,
            read_device_id_code,
            start_object_id,
            out,
        );
        Err(MbusError::InvalidFunctionCode)
    }
}

// ---------------------------------------------------------------------------
// Conditional traffic requirement — auto-satisfied when traffic is disabled.
// ---------------------------------------------------------------------------

/// Internal trait that requires [`TrafficNotifier`] when the `traffic` feature is
/// active; otherwise every type satisfies it automatically.
#[doc(hidden)]
#[cfg(not(feature = "traffic"))]
pub trait AppRequirements {}
#[cfg(not(feature = "traffic"))]
impl<T> AppRequirements for T {}

#[doc(hidden)]
#[cfg(feature = "traffic")]
pub trait AppRequirements: TrafficNotifier {}
#[cfg(feature = "traffic")]
impl<T: TrafficNotifier> AppRequirements for T {}

// ---------------------------------------------------------------------------
// Composed supertrait + blanket impl
// ---------------------------------------------------------------------------

/// Composed supertrait that the server runtime binds on.
///
/// You never need to implement this trait directly — it is **automatically satisfied**
/// when your type implements the individual split traits plus [`AppRequirements`].
///
/// ```ignore
/// impl ServerExceptionHandler for MyApp {}
/// impl ServerCoilHandler for MyApp { /* ... */ }
/// impl ServerHoldingRegisterHandler for MyApp { /* ... */ }
/// // … empty impls for unused handlers …
///
/// // ModbusAppHandler is now auto-implemented for MyApp.
/// ```
pub trait ModbusAppHandler:
    ServerExceptionHandler
    + AppRequirements
    + ServerCoilHandler
    + ServerDiscreteInputHandler
    + ServerHoldingRegisterHandler
    + ServerInputRegisterHandler
    + ServerFifoHandler
    + ServerFileRecordHandler
    + ServerDiagnosticsHandler
{
}

impl<T> ModbusAppHandler for T where
    T: ServerExceptionHandler
        + AppRequirements
        + ServerCoilHandler
        + ServerDiscreteInputHandler
        + ServerHoldingRegisterHandler
        + ServerInputRegisterHandler
        + ServerFifoHandler
        + ServerFileRecordHandler
        + ServerDiagnosticsHandler
{
}

/// Abstraction for acquiring temporary mutable access to a Modbus application.
///
/// This trait intentionally does not prescribe a specific synchronization primitive.
/// Users can implement it with:
/// - `std` mutexes/locks on desktop or server targets,
/// - RTOS mutexes/semaphores,
/// - bare-metal critical sections or interior mutability,
/// - or a simple direct mutable owner in single-threaded systems.
///
/// The closure-based API ties the mutable borrow lifetime to the critical section,
/// preventing accidental lock leaks and keeping usage safe across panic/error paths.
pub trait ModbusAppAccess {
    /// Concrete application model type generated (or implemented) as [`ModbusAppHandler`].
    type App: ModbusAppHandler;

    /// Executes `f` with exclusive mutable access to the underlying app instance.
    ///
    /// Implementors decide how exclusivity is guaranteed (lock, critical section,
    /// scheduler primitive, or single-threaded ownership discipline).
    fn with_app_mut<R, F>(&self, f: F) -> R
    where
        F: FnOnce(&mut Self::App) -> R;
}

/// Forwarding wrapper that adapts any [`ModbusAppAccess`] into [`ModbusAppHandler`].
///
/// This is the recommended way to connect shared or runtime-owned app state to
/// `ServerServices` without writing repetitive method delegation boilerplate.
///
/// # Typical usage
/// 1. Derive `modbus_app` on your concrete app model so it implements [`ModbusAppHandler`].
/// 2. Create a small runtime wrapper that implements [`ModbusAppAccess`].
/// 3. Wrap it with `ForwardingApp::new(...)` and pass it to `ServerServices`.
#[derive(Debug, Clone)]
pub struct ForwardingApp<A> {
    inner: A,
}

impl<A> ForwardingApp<A> {
    /// Creates a forwarding adapter from a user-provided access wrapper.
    pub fn new(inner: A) -> Self {
        Self { inner }
    }

    /// Returns a shared reference to the inner access wrapper.
    pub fn get_ref(&self) -> &A {
        &self.inner
    }

    /// Returns a mutable reference to the inner access wrapper.
    pub fn get_mut(&mut self) -> &mut A {
        &mut self.inner
    }

    /// Consumes the adapter and returns the wrapped access object.
    pub fn into_inner(self) -> A {
        self.inner
    }
}

#[cfg(feature = "traffic")]
impl<A: ModbusAppAccess> TrafficNotifier for ForwardingApp<A> {
    fn on_rx_frame(&mut self, txn_id: u16, unit_id_or_slave_addr: UnitIdOrSlaveAddr, frame: &[u8]) {
        self.inner
            .with_app_mut(|app| app.on_rx_frame(txn_id, unit_id_or_slave_addr, frame))
    }

    fn on_tx_frame(&mut self, txn_id: u16, unit_id_or_slave_addr: UnitIdOrSlaveAddr, frame: &[u8]) {
        self.inner
            .with_app_mut(|app| app.on_tx_frame(txn_id, unit_id_or_slave_addr, frame))
    }

    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        self.inner
            .with_app_mut(|app| app.on_rx_error(txn_id, unit_id_or_slave_addr, error, frame))
    }

    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
        frame: &[u8],
    ) {
        self.inner
            .with_app_mut(|app| app.on_tx_error(txn_id, unit_id_or_slave_addr, error, frame))
    }
}
impl<A> ServerExceptionHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn on_exception(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        function_code: FunctionCode,
        exception_code: ExceptionCode,
        error: MbusError,
    ) {
        self.inner.with_app_mut(|app| {
            app.on_exception(
                txn_id,
                unit_id_or_slave_addr,
                function_code,
                exception_code,
                error,
            )
        })
    }
}

impl<A> ServerCoilHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_coils_request(txn_id, unit_id_or_slave_addr, address, quantity, out)
        })
    }

    fn write_single_coil_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.write_single_coil_request(txn_id, unit_id_or_slave_addr, address, value)
        })
    }

    fn write_multiple_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
        values: &[u8],
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.write_multiple_coils_request(
                txn_id,
                unit_id_or_slave_addr,
                starting_address,
                quantity,
                values,
            )
        })
    }
}

impl<A> ServerDiscreteInputHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_discrete_inputs_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_discrete_inputs_request(txn_id, unit_id_or_slave_addr, address, quantity, out)
        })
    }
}

impl<A> ServerHoldingRegisterHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_multiple_holding_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_multiple_holding_registers_request(
                txn_id,
                unit_id_or_slave_addr,
                address,
                quantity,
                out,
            )
        })
    }

    fn write_single_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.write_single_register_request(txn_id, unit_id_or_slave_addr, address, value)
        })
    }

    fn write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        values: &[u16],
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.write_multiple_registers_request(
                txn_id,
                unit_id_or_slave_addr,
                starting_address,
                values,
            )
        })
    }

    fn mask_write_register_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.mask_write_register_request(
                txn_id,
                unit_id_or_slave_addr,
                address,
                and_mask,
                or_mask,
            )
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn read_write_multiple_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_write_multiple_registers_request(
                txn_id,
                unit_id_or_slave_addr,
                read_address,
                read_quantity,
                write_address,
                write_values,
                out,
            )
        })
    }
}

impl<A> ServerInputRegisterHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_multiple_input_registers_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_multiple_input_registers_request(
                txn_id,
                unit_id_or_slave_addr,
                address,
                quantity,
                out,
            )
        })
    }
}

impl<A> ServerFifoHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_fifo_queue_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        pointer_address: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_fifo_queue_request(txn_id, unit_id_or_slave_addr, pointer_address, out)
        })
    }
}

impl<A> ServerFileRecordHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        out: &mut [u8],
    ) -> Result<u8, MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_file_record_request(
                txn_id,
                unit_id_or_slave_addr,
                file_number,
                record_number,
                record_length,
                out,
            )
        })
    }

    fn write_file_record_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        file_number: u16,
        record_number: u16,
        record_length: u16,
        record_data: &[u16],
    ) -> Result<(), MbusError> {
        self.inner.with_app_mut(|app| {
            app.write_file_record_request(
                txn_id,
                unit_id_or_slave_addr,
                file_number,
                record_number,
                record_length,
                record_data,
            )
        })
    }
}

impl<A> ServerDiagnosticsHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    fn read_exception_status_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<u8, MbusError> {
        self.inner
            .with_app_mut(|app| app.read_exception_status_request(txn_id, unit_id_or_slave_addr))
    }

    fn diagnostics_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        sub_function: mbus_core::function_codes::public::DiagnosticSubFunction,
        data: u16,
    ) -> Result<u16, MbusError> {
        self.inner.with_app_mut(|app| {
            app.diagnostics_request(txn_id, unit_id_or_slave_addr, sub_function, data)
        })
    }

    fn get_comm_event_counter_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
    ) -> Result<(u16, u16), MbusError> {
        self.inner
            .with_app_mut(|app| app.get_comm_event_counter_request(txn_id, unit_id_or_slave_addr))
    }

    fn get_comm_event_log_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_events: &mut [u8],
    ) -> Result<(u16, u16, u16, u8), MbusError> {
        self.inner.with_app_mut(|app| {
            app.get_comm_event_log_request(txn_id, unit_id_or_slave_addr, out_events)
        })
    }

    fn report_server_id_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        out_server_id: &mut [u8],
    ) -> Result<(u8, u8), MbusError> {
        self.inner.with_app_mut(|app| {
            app.report_server_id_request(txn_id, unit_id_or_slave_addr, out_server_id)
        })
    }

    fn read_device_identification_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        read_device_id_code: u8,
        start_object_id: u8,
        out: &mut [u8],
    ) -> Result<(u8, u8, bool, u8), MbusError> {
        self.inner.with_app_mut(|app| {
            app.read_device_identification_request(
                txn_id,
                unit_id_or_slave_addr,
                read_device_id_code,
                start_object_id,
                out,
            )
        })
    }
}
