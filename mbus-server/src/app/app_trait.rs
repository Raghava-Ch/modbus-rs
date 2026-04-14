//! Application Layer Traits
//!
//! This module defines the core traits used to bridge the Modbus protocol stack with
//! user-defined application logic. It follows a callback-based (observer) pattern
//! where the stack notifies the application of successful responses or failures.
//!
//! Each trait corresponds to a functional group of Modbus services (Coils, Registers, etc.).
//!
//! ## Callback Contract (applies to all traits in this file)
//!
//! - Callbacks are dispatched from `ClientServices::poll()`. No callback is invoked unless
//!   the application actively calls `poll()`.
//! - A successful callback means the response was fully parsed and validated against the
//!   queued request context (transaction id, unit/slave address, and operation metadata).
//! - For a single request, either:
//!   - one success callback is invoked from the corresponding response trait, or
//!   - one failure callback is invoked via [`RequestErrorNotifier::request_failed`].
//! - After either callback path runs, the request is removed from the internal queue.
//! - Callback implementations should remain lightweight and non-blocking. If heavy work is
//!   needed (database writes, UI updates, IPC), enqueue that work into your own task queue.
//! - `txn_id` is always the original id supplied by the caller, including Serial modes where
//!   transaction ids are not transmitted on the wire.

use mbus_core::{errors::MbusError, transport::UnitIdOrSlaveAddr};

#[cfg(feature = "coils")]
use crate::Coils;
// #[cfg(feature = "diagnostics")]
// use crate::DeviceIdentificationResponse;
// #[cfg(feature = "discrete-inputs")]
// use crate::DiscreteInputs;
// #[cfg(feature = "fifo")]
// use crate::FifoQueue;
// #[cfg(feature = "file-record")]
// use crate::SubRequestParams;
#[cfg(feature = "traffic")]
/// Direction of raw Modbus frame traffic observed by the client stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrafficDirection {
    /// Outgoing request ADU sent by the client.
    Tx,
    /// Incoming response ADU received by the client.
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
    fn on_tx_frame(&mut self, _txn_id: u16, _unit_id_or_slave_addr: UnitIdOrSlaveAddr) {}

    /// Called when an incoming request frame is accepted for dispatch.
    fn on_rx_frame(&mut self, _txn_id: u16, _unit_id_or_slave_addr: UnitIdOrSlaveAddr) {}

    /// Called when sending a response frame failed.
    fn on_tx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
    ) {
    }

    /// Called when processing an incoming request frame failed.
    fn on_rx_error(
        &mut self,
        _txn_id: u16,
        _unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        _error: MbusError,
    ) {
    }
}

/// Trait defining the expected response handling for coil-related Modbus operations.
///
/// Implementors of this trait to deliver the responses to the application layer,
/// allowing application developers to process the coil data and update their application state accordingly.
///
/// ## When Each Callback Is Fired
/// - `read_coils_response`: after a successful FC 0x01 response for a multi-coil read.
/// - `read_single_coil_response`: convenience callback when quantity was 1.
/// - `write_single_coil_response`: after a successful FC 0x05 echo/ack response.
/// - `write_multiple_coils_response`: after a successful FC 0x0F response containing
///   start address and quantity written by the server.
///
/// ## Data Semantics
/// - Address values are Modbus data-model addresses exactly as acknowledged by the server.
/// - Boolean coil values follow Modbus conventions: `true` = ON (`0xFF00` in FC 0x05 request),
///   `false` = OFF (`0x0000`).
#[cfg(feature = "coils")]
pub trait CoilRequest {
    /// Handles a Read Coils request by invoking the appropriate application callback with the coil states.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `coils`: A wrapper containing the bit-packed boolean statuses of the requested coils.
    fn read_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        coils: &Coils,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, coils);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a Read Single Coil request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The exact address of the single coil that was read.
    /// - `value`: The boolean state of the coil (`true` = ON, `false` = OFF).
    fn read_single_coil_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, value);
        Err(MbusError::InvalidFunctionCode)
    }

    /// Handles a Write Single Coil request, confirming the state change.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the coil that was successfully written.
    /// - `value`: The boolean state applied to the coil (`true` = ON, `false` = OFF).
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

    /// Handles a Write Multiple Coils request, confirming the bulk state change.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address where the bulk write began.
    /// - `quantity`: The total number of consecutive coils updated.
    fn write_multiple_coils_request(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        let _ = (txn_id, unit_id_or_slave_addr, address, quantity);
        Err(MbusError::InvalidFunctionCode)
    }
}

// Trait defining the expected request handling for FIFO Queue Modbus operations.
//
// ## When Callback Is Fired
// - `read_fifo_queue_request` is invoked after a successful FC 0x18 request.
//
// ## Data Semantics
// - `fifo_queue` contains values in server-returned order.
// - Quantity in the payload may vary between calls depending on device state.
//
// ## Implementation Guidance
//   non-blocking because it runs in the `poll()` execution path.
// #[cfg(feature = "fifo")]
// pub trait FifoQueueRequest {
//     /// Handles a Read FIFO Queue request.

//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The unit ID of the device that responded.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `fifo_queue`: A `FifoQueue` struct containing the values pulled from the queue.
//     fn read_fifo_queue_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         fifo_queue: &FifoQueue,
//     );
// }

// /// Trait defining the expected request handling for File Record Modbus operations.
// ///
// /// ## When Each Callback Is Fired
// /// - `read_file_record_request`: after successful FC 0x14 request parsing.
// /// - `write_file_record_request`: after successful FC 0x15 acknowledgement.
// ///
// /// ## Data Semantics
// /// - For read requests, each `SubRequestParams` entry reflects one returned record chunk.
// /// - Per Modbus spec, the request does not echo `file_number` or `record_number`; those
// ///   fields are therefore reported as `0` in callback data and should not be used as identity.
// #[cfg(feature = "file-record")]
// pub trait FileRecordRequest {
//     /// Handles a Read File Record request.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `data`: A slice containing the sub-request responses. Note that `file_number` and `record_number`
//     ///
//     /// are not returned by the server in the request PDU and will be set to 0 in the parameters.
//     fn read_file_record_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         data: &[SubRequestParams],
//     );

//     /// Handles a Write File Record request, confirming the write was successful.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     fn write_file_record_request(&mut self, txn_id: u16, unit_id_or_slave_addr: UnitIdOrSlaveAddr);
// }

/// Defines callbacks for handling requests to Modbus register-related operations.
/// Internal trait that aggregates conditional requirements for [`ModbusAppHandler`].
///
/// When the `traffic` feature is active this also requires [`TrafficNotifier`] to
/// be implemented; otherwise every type satisfies it automatically.
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

/// Defines callbacks for handling requests to Modbus register-related operations.
///
/// Implementors of this trait provide the storage and routing logic used by the
/// server when it receives register or coil requests.
///
/// ## Callback Mapping
/// - FC 0x01: `read_coils_request`
/// - FC 0x02: `read_discrete_inputs_request`
/// - FC 0x03: `read_multiple_holding_registers_request`
/// - FC 0x04: `read_multiple_input_registers_request`
/// - FC 0x05: `write_single_coil_request`
/// - FC 0x06: `write_single_register_request`
/// - FC 0x0F: `write_multiple_coils_request`
/// - FC 0x10: `write_multiple_registers_request`
/// - FC 0x16: `mask_write_register_request`
///
/// ## Data Semantics
/// - Register reads write big-endian wire bytes into `out` and return the byte count written.
/// - Register writes receive already-decoded `u16` values.
/// - Coil reads write packed Modbus coil bytes into `out` and return the packed byte count.
/// - Multi-write coil requests pass the original packed request bytes plus the validated quantity.
pub trait ModbusAppHandler: AppRequirements {
    /// Handles a `Read Coils` (FC 0x01) request.
    #[cfg(feature = "coils")]
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

    /// Handles a `Read Discrete Inputs` (FC 0x02) request.
    #[cfg(feature = "discrete-inputs")]
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

    /// Handles a `Read Input Registers` (FC 0x04) request.
    #[cfg(feature = "input-registers")]
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

    /// Handles a `Read Holding Registers` (FC 0x03) request.
    ///
    /// The stack provides `out` — a stack-allocated byte buffer large enough for
    /// the full Modbus PDU data payload (up to 252 bytes). The implementation
    /// must write big-endian register words starting at `out[0]` and return the
    /// total byte count written.  The buffer is owned by `ServerServices` and is
    /// only live for the duration of `poll()`.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request.
    /// - `unit_id_or_slave_addr`: Unit ID (TCP) or slave address (Serial).
    /// - `address`: Starting register address from the FC 0x03 request.
    /// - `quantity`: Number of registers requested.
    /// - `out`: Mutable byte buffer to write wire-format register data into.
    ///
    /// # Returns
    /// - `Ok(n)` — `n` bytes written into `out` (must be `quantity * 2`).
    /// - `Err(MbusError)` — the server emits a function-specific exception response.
    #[cfg(feature = "holding-registers")]
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

    /// Handles a `Write Single Coil` (FC 0x05) request.
    #[cfg(feature = "coils")]
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

    /// Handles a `Write Single Register` (FC 0x06) request.
    #[cfg(feature = "holding-registers")]
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

    /// Handles a `Write Multiple Coils` (FC 0x0F) request.
    #[cfg(feature = "coils")]
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

    /// Handles a `Write Multiple Registers` (FC 0x10) request.
    #[cfg(feature = "holding-registers")]
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
    #[cfg(feature = "holding-registers")]
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
    fn on_rx_frame(&mut self, txn_id: u16, unit_id_or_slave_addr: UnitIdOrSlaveAddr) {
        self.inner
            .with_app_mut(|app| app.on_rx_frame(txn_id, unit_id_or_slave_addr))
    }

    fn on_tx_frame(&mut self, txn_id: u16, unit_id_or_slave_addr: UnitIdOrSlaveAddr) {
        self.inner
            .with_app_mut(|app| app.on_tx_frame(txn_id, unit_id_or_slave_addr))
    }

    fn on_rx_error(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
    ) {
        self.inner
            .with_app_mut(|app| app.on_rx_error(txn_id, unit_id_or_slave_addr, error))
    }

    fn on_tx_error(
        &mut self,
        txn_id: u16,
        unit_id_or_slave_addr: UnitIdOrSlaveAddr,
        error: MbusError,
    ) {
        self.inner
            .with_app_mut(|app| app.on_tx_error(txn_id, unit_id_or_slave_addr, error))
    }
}

impl<A> ModbusAppHandler for ForwardingApp<A>
where
    A: ModbusAppAccess,
{
    #[cfg(feature = "coils")]
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

    #[cfg(feature = "input-registers")]
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

    #[cfg(feature = "holding-registers")]
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

    #[cfg(feature = "coils")]
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

    #[cfg(feature = "holding-registers")]
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

    #[cfg(feature = "coils")]
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

    #[cfg(feature = "holding-registers")]
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
}

// /// Defines callbacks for handling requests to Modbus discrete input-related requests.
// ///
// /// Implementors of this trait can process the data received from a Modbus server
// /// and update their application state accordingly.
// ///
// /// ## When Each Callback Is Fired
// /// - `read_multiple_discrete_inputs_request`: after successful FC 0x02 with quantity > 1.
// /// - `read_single_discrete_input_request`: convenience callback when quantity was 1.
// ///
// /// ## Data Semantics
// /// - `DiscreteInputs` stores bit-packed values; use helper methods on the type instead of
// ///   manually decoding bit offsets in application code.
// #[cfg(feature = "discrete-inputs")]
// pub trait DiscreteInputResponse {
//     /// Handles a request for a `Read Discrete Inputs` (FC 0x02) request.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The unit ID of the device that responded.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `discrete_inputs`: A `DiscreteInputs` struct containing the states of the read inputs.
//     fn read_multiple_discrete_inputs_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         discrete_inputs: &DiscreteInputs,
//     );

//     /// Handles a request for a single discrete input read.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The unit ID of the device that responded.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `address`: The address of the input that was read.
//     /// - `value`: The boolean state of the read input.
//     fn read_single_discrete_input_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         address: u16,
//         value: bool,
//     );
// }

// /// Trait for handling Diagnostics-family requests.
// ///
// /// ## Callback Mapping
// /// - FC 0x2B / MEI 0x0E: `read_device_identification_request`
// /// - FC 0x2B / other MEI: `encapsulated_interface_transport_request`
// /// - FC 0x07: `read_exception_status_request`
// /// - FC 0x08: `diagnostics_request`
// /// - FC 0x0B: `get_comm_event_counter_request`
// /// - FC 0x0C: `get_comm_event_log_request`
// /// - FC 0x11: `report_server_id_request`
// ///
// /// ## Data Semantics
// /// - `mei_type`, `sub_function`, counters, and event buffers are already validated and decoded.
// /// - Large payloads (event logs, generic encapsulated transport data) should typically be copied
// ///   or forwarded quickly, then processed outside the callback hot path.
// #[cfg(feature = "diagnostics")]
// pub trait DiagnosticsRequest {
//     /// Called when a Read Device Identification request is received.
//     ///
//     /// Implementors can use this callback to process the device identity info (Vendor, Product Code, etc.).
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The unit ID of the device that responded.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `request`: Extracted device identification strings.
//     fn read_device_identification_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         request: &DeviceIdentificationRequest,
//     );

//     /// Called when a generic Encapsulated Interface Transport request (FC 43) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The unit ID of the device that responded.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `mei_type`: The MEI type returned in the request.
//     /// - `data`: The data payload returned in the request.
//     fn encapsulated_interface_transport_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         mei_type: EncapsulatedInterfaceType,
//         data: &[u8],
//     );

//     /// Called when a Read Exception Status request (FC 07) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `status`: The 8-bit exception status code returned by the server.
//     fn read_exception_status_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         status: u8,
//     );

//     /// Called when a Diagnostics request (FC 08) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `sub_function`: The sub-function code confirming the diagnostic test.
//     /// - `data`: Data payload returned by the diagnostic test (e.g., echoed loopback data).
//     fn diagnostics_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         sub_function: DiagnosticSubFunction,
//         data: &[u16],
//     );

//     /// Called when a Get Comm Event Counter request (FC 11) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `status`: The status word indicating if the device is busy.
//     /// - `event_count`: The number of successful messages processed by the device.
//     fn get_comm_event_counter_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         status: u16,
//         event_count: u16,
//     );

//     /// Called when a Get Comm Event Log request (FC 12) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `status`: The status word indicating device state.
//     /// - `event_count`: Number of successful messages processed.
//     /// - `message_count`: Quantity of messages processed since the last restart.
//     /// - `events`: Raw byte array containing the device's internal event log.
//     fn get_comm_event_log_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         status: u16,
//         event_count: u16,
//         message_count: u16,
//         events: &[u8],
//     );

//     /// Called when a Report Server ID request (FC 17) is received.
//     ///
//     /// # Parameters
//     /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
//     ///   does not natively use transaction IDs, the stack preserves the ID provided in
//     ///   the request and returns it here to allow for asynchronous tracking.
//     /// - `unit_id_or_slave_addr`: The target Modbus unit ID or slave address.
//     ///   - `unit_id`: if transport is tcp
//     ///   - `slave_addr`: if transport is serial
//     /// - `data`: Raw identity/status data provided by the manufacturer.
//     fn report_server_id_request(
//         &mut self,
//         txn_id: u16,
//         unit_id_or_slave_addr: UnitIdOrSlaveAddr,
//         data: &[u8],
//     );
// }
