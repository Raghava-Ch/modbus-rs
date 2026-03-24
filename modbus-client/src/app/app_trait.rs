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

use mbus_core::{
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    transport::UnitIdOrSlaveAddr,
};

#[cfg(feature = "coils")]
use crate::services::coil::Coils;
#[cfg(feature = "diagnostics")]
use crate::services::diagnostic::DeviceIdentificationResponse;
#[cfg(feature = "discrete-inputs")]
use crate::services::discrete_input::DiscreteInputs;
#[cfg(feature = "fifo")]
use crate::services::fifo_queue::FifoQueue;
#[cfg(feature = "file-record")]
use crate::services::file_record::SubRequestParams;
#[cfg(feature = "registers")]
use crate::services::register::Registers;

/// Trait for receiving notifications about failed Modbus requests.
///
/// This is used to handle timeouts, connection issues, or Modbus exception responses
/// at the application level, allowing the implementor to gracefully recover or alert the user.
pub trait RequestErrorNotifier {
    /// Called by the client stack whenever a previously queued request cannot be completed.
    ///
    /// The `error` parameter identifies the exact failure cause. The following variants are
    /// delivered by the stack's internal `poll()` and `handle_timeouts()` logic:
    ///
    /// - **`MbusError::ModbusException(code)`** — The remote device replied with a Modbus
    ///   exception frame (`function code 0x80 + FC`). The server understood the request but
    ///   refused to execute it (e.g. illegal data address, illegal function). Delivered
    ///   immediately inside the `poll()` call that received the exception response, before
    ///   any retry logic runs.
    ///
    /// - **`MbusError::NoRetriesLeft`** — The response timeout expired and every configured
    ///   retry attempt was exhausted. `handle_timeouts()` waits `response_timeout_ms`
    ///   milliseconds after each send, schedules each retry according to the configured
    ///   `BackoffStrategy` and `JitterStrategy`, and fires this error only after the last
    ///   retry attempt has itself timed out without a response. The request is permanently
    ///   removed from the queue.
    ///
    /// - **`MbusError::SendFailed`** — A scheduled retry was due (its backoff timestamp was
    ///   reached inside `handle_timeouts()`), but the call to `transport.send()` returned an
    ///   error (e.g. the TCP connection or serial port was lost between the original send and
    ///   the retry). The request is dropped immediately; remaining retries in the budget are
    ///   not consumed.
    ///
    /// # Notes
    /// - Each call corresponds to exactly one transaction. After this call the request is
    ///   permanently removed from the internal expected-response queue and will not be retried
    ///   again. No further callbacks will be issued for the same `txn_id`.
    /// - The `txn_id` is always the value supplied when the request was originally enqueued,
    ///   even for Serial transports that do not transmit a transaction ID on the wire.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request.
    /// - `unit_id_slave_addr`: The target Modbus unit ID (TCP) or slave address (Serial).
    /// - `error`: The specific [`MbusError`] variant describing the failure (see above).
    fn request_failed(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr, error: MbusError);
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
pub trait CoilResponse {
    /// Handles a Read Coils response by invoking the appropriate application callback with the coil states.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `coils`: A wrapper containing the bit-packed boolean statuses of the requested coils.
    fn read_coils_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        coils: &Coils,
    );

    /// Handles a Read Single Coil response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The exact address of the single coil that was read.
    /// - `value`: The boolean state of the coil (`true` = ON, `false` = OFF).
    fn read_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    );

    /// Handles a Write Single Coil response, confirming the state change.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the coil that was successfully written.
    /// - `value`: The boolean state applied to the coil (`true` = ON, `false` = OFF).
    fn write_single_coil_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    );

    /// Handles a Write Multiple Coils response, confirming the bulk state change.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The starting address where the bulk write began.
    /// - `quantity`: The total number of consecutive coils updated.
    fn write_multiple_coils_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    );
}

/// Trait defining the expected response handling for FIFO Queue Modbus operations.
///
/// ## When Callback Is Fired
/// - `read_fifo_queue_response` is invoked after a successful FC 0x18 response.
///
/// ## Data Semantics
/// - `fifo_queue` contains values in server-returned order.
/// - Quantity in the payload may vary between calls depending on device state.
///
/// ## Implementation Guidance
///   non-blocking because it runs in the `poll()` execution path.
#[cfg(feature = "fifo")]
pub trait FifoQueueResponse {
    /// Handles a Read FIFO Queue response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `fifo_queue`: A `FifoQueue` struct containing the values pulled from the queue.
    fn read_fifo_queue_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        fifo_queue: &FifoQueue,
    );
}

/// Trait defining the expected response handling for File Record Modbus operations.
///
/// ## When Each Callback Is Fired
/// - `read_file_record_response`: after successful FC 0x14 response parsing.
/// - `write_file_record_response`: after successful FC 0x15 acknowledgement.
///
/// ## Data Semantics
/// - For read responses, each `SubRequestParams` entry reflects one returned record chunk.
/// - Per Modbus spec, the response does not echo `file_number` or `record_number`; those
///   fields are therefore reported as `0` in callback data and should not be used as identity.
#[cfg(feature = "file-record")]
pub trait FileRecordResponse {
    /// Handles a Read File Record response.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `data`: A slice containing the sub-request responses. Note that `file_number` and `record_number`
    /// 
    /// are not returned by the server in the response PDU and will be set to 0 in the parameters.
    fn read_file_record_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[SubRequestParams],
    );

    /// Handles a Write File Record response, confirming the write was successful.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    fn write_file_record_response(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr);
}

/// Defines callbacks for handling responses to Modbus register-related requests.
///
/// Implementors of this trait can process the data received from a Modbus server
/// and update their application state accordingly. Each method corresponds to a
/// specific Modbus register operation response.
///
/// ## Callback Mapping
/// - FC 0x03: `read_multiple_holding_registers_response`, `read_single_holding_register_response`
/// - FC 0x04: `read_multiple_input_registers_response`, `read_single_input_register_response`
/// - FC 0x06: `write_single_register_response`
/// - FC 0x10: `write_multiple_registers_response`
/// - FC 0x16: `mask_write_register_response`
/// - FC 0x17: `read_write_multiple_registers_response`
///
/// ## Data Semantics
/// - Register values are 16-bit words (`u16`) already decoded from Modbus big-endian byte pairs.
/// - Address and quantity values are echoed/validated values corresponding to the original request.
#[cfg(feature = "registers")]
pub trait RegisterResponse {
    /// Handles a response for a `Read Input Registers` (FC 0x04) request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `registers`: A `Registers` struct containing the values of the read input registers.
    fn read_multiple_input_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    );

    /// Handles a response for a `Read Single Input Register` (FC 0x04) request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the register that was read.
    /// - `value`: The value of the read register.
    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Read Holding Registers` (FC 0x03) request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `registers`: A `Registers` struct containing the values of the read holding registers.
    fn read_multiple_holding_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    );

    /// Handles a response for a `Write Single Register` (FC 0x06) request, confirming a successful write.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the register that was written.
    /// - `value`: The value that was written to the register.
    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Write Multiple Registers` (FC 0x10) request, confirming a successful write.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `starting_address`: The starting address of the registers that were written.
    /// - `quantity`: The number of registers that were written.
    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        starting_address: u16,
        quantity: u16,
    );

    /// Handles a response for a `Read/Write Multiple Registers` (FC 0x17) request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `registers`: A `Registers` struct containing the values of the registers that were read.
    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        registers: &Registers,
    );

    /// Handles a response for a single register read request.
    ///
    /// This is a convenience callback for when only one register is requested.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the register that was read.
    /// - `value`: The value of the read register.
    fn read_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    );

    /// Handles a response for a single holding register write request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the register that was written.
    /// - `value`: The value that was written to the register.
    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Mask Write Register` (FC 0x16) request, confirming a successful operation.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    fn mask_write_register_response(&mut self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr);
}

/// Defines callbacks for handling responses to Modbus discrete input-related requests.
///
/// Implementors of this trait can process the data received from a Modbus server
/// and update their application state accordingly.
///
/// ## When Each Callback Is Fired
/// - `read_multiple_discrete_inputs_response`: after successful FC 0x02 with quantity > 1.
/// - `read_single_discrete_input_response`: convenience callback when quantity was 1.
///
/// ## Data Semantics
/// - `DiscreteInputs` stores bit-packed values; use helper methods on the type instead of
///   manually decoding bit offsets in application code.
#[cfg(feature = "discrete-inputs")]
pub trait DiscreteInputResponse {
    /// Handles a response for a `Read Discrete Inputs` (FC 0x02) request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `discrete_inputs`: A `DiscreteInputs` struct containing the states of the read inputs.
    fn read_multiple_discrete_inputs_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        discrete_inputs: &DiscreteInputs,
    );

    /// Handles a response for a single discrete input read request.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `address`: The address of the input that was read.
    /// - `value`: The boolean state of the read input.
    fn read_single_discrete_input_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        value: bool,
    );
}

/// Trait for handling Diagnostics-family responses.
///
/// ## Callback Mapping
/// - FC 0x2B / MEI 0x0E: `read_device_identification_response`
/// - FC 0x2B / other MEI: `encapsulated_interface_transport_response`
/// - FC 0x07: `read_exception_status_response`
/// - FC 0x08: `diagnostics_response`
/// - FC 0x0B: `get_comm_event_counter_response`
/// - FC 0x0C: `get_comm_event_log_response`
/// - FC 0x11: `report_server_id_response`
///
/// ## Data Semantics
/// - `mei_type`, `sub_function`, counters, and event buffers are already validated and decoded.
/// - Large payloads (event logs, generic encapsulated transport data) should typically be copied
///   or forwarded quickly, then processed outside the callback hot path.
#[cfg(feature = "diagnostics")]
pub trait DiagnosticsResponse {
    /// Called when a Read Device Identification response is received.
    ///
    /// Implementors can use this callback to process the device identity info (Vendor, Product Code, etc.).
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `response`: Extracted device identification strings.
    fn read_device_identification_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        response: &DeviceIdentificationResponse,
    );

    /// Called when a generic Encapsulated Interface Transport response (FC 43) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The unit ID of the device that responded.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `mei_type`: The MEI type returned in the response.
    /// - `data`: The data payload returned in the response.
    fn encapsulated_interface_transport_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        mei_type: EncapsulatedInterfaceType,
        data: &[u8],
    );

    /// Called when a Read Exception Status response (FC 07) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `status`: The 8-bit exception status code returned by the server.
    fn read_exception_status_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u8,
    );

    /// Called when a Diagnostics response (FC 08) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `sub_function`: The sub-function code confirming the diagnostic test.
    /// - `data`: Data payload returned by the diagnostic test (e.g., echoed loopback data).
    fn diagnostics_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        sub_function: DiagnosticSubFunction,
        data: &[u16],
    );

    /// Called when a Get Comm Event Counter response (FC 11) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `status`: The status word indicating if the device is busy.
    /// - `event_count`: The number of successful messages processed by the device.
    fn get_comm_event_counter_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
    );

    /// Called when a Get Comm Event Log response (FC 12) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `status`: The status word indicating device state.
    /// - `event_count`: Number of successful messages processed.
    /// - `message_count`: Quantity of messages processed since the last restart.
    /// - `events`: Raw byte array containing the device's internal event log.
    fn get_comm_event_log_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        status: u16,
        event_count: u16,
        message_count: u16,
        events: &[u8],
    );

    /// Called when a Report Server ID response (FC 17) is received.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial
    /// - `data`: Raw identity/status data provided by the manufacturer.
    fn report_server_id_response(
        &mut self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[u8],
    );
}
