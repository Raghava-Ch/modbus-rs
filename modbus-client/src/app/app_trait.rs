//! Application Layer Traits
//!
//! This module defines the core traits used to bridge the Modbus protocol stack with
//! user-defined application logic. It follows a callback-based (observer) pattern
//! where the stack notifies the application of successful responses or failures.
//!
//! Each trait corresponds to a functional group of Modbus services (Coils, Registers, etc.).

use crate::services::{
    coil::Coils, diagnostic::DeviceIdentificationResponse, discrete_input::DiscreteInputs,
    fifo_queue::FifoQueue, file_record::SubRequestParams, register::Registers,
};
use mbus_core::{
    errors::MbusError,
    function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType},
    transport::UnitIdOrSlaveAddr,
};

/// Trait for receiving notifications about failed Modbus requests.
///
/// This is used to handle timeouts, connection issues, or Modbus exception responses
/// at the application level, allowing the implementor to gracefully recover or alert the user.
pub trait RequestErrorNotifier {
    /// Handles a failed request by invoking the appropriate application callback with the error information.
    ///
    /// This method is invoked when:
    /// - A Modbus device returns an Exception response.
    /// - The request times out (after all configured retries are exhausted).
    /// - The underlying transport connection drops.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `error`: The specific `MbusError` detailing why the request failed.
    fn request_failed(&self, txn_id: u16, unit_id_slave_addr: UnitIdOrSlaveAddr, error: MbusError);
}

/// Trait defining the expected response handling for coil-related Modbus operations.
///
/// Implementors of this trait to deliver the responses to the application layer,
/// allowing application developers to process the coil data and update their application state accordingly.
pub trait CoilResponse {
    /// Handles a Read Coils response by invoking the appropriate application callback with the coil states.
    ///
    /// # Parameters
    /// - `txn_id`: Transaction ID of the original request. While Modbus Serial (RTU/ASCII)
    ///   does not natively use transaction IDs, the stack preserves the ID provided in
    ///   the request and returns it here to allow for asynchronous tracking.
    /// - `unit_id_slave_addr`: The target Modbus unit ID or slave address.
    ///   - `unit_id`: if transport is tcp
    ///   - `slave_addr`: if transport is serial/// - `coils`: A wrapper containing the bit-packed boolean statuses of the requested coils.
    fn read_coils_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `address`: The exact address of the single coil that was read.
    /// - `value`: The boolean state of the coil (`true` = ON, `false` = OFF).
    fn read_single_coil_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `address`: The address of the coil that was successfully written.
    /// - `value`: The boolean state applied to the coil (`true` = ON, `false` = OFF).
    fn write_single_coil_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `address`: The starting address where the bulk write began.
    /// - `quantity`: The total number of consecutive coils updated.
    fn write_multiple_coils_response(
        &self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        address: u16,
        quantity: u16,
    );
}

/// Trait defining the expected response handling for FIFO Queue Modbus operations.
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

/// Trait for handling Diagnostics responses.
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
        &self,
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
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `status`: The 8-bit exception status code returned by the server.
    fn read_exception_status_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `sub_function`: The sub-function code confirming the diagnostic test.
    /// - `data`: Data payload returned by the diagnostic test (e.g., echoed loopback data).
    fn diagnostics_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `status`: The status word indicating if the device is busy.
    /// - `event_count`: The number of successful messages processed by the device.
    fn get_comm_event_counter_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `status`: The status word indicating device state.
    /// - `event_count`: Number of successful messages processed.
    /// - `message_count`: Quantity of messages processed since the last restart.
    /// - `events`: Raw byte array containing the device's internal event log.
    fn get_comm_event_log_response(
        &self,
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
    ///   - `slave_addr`: if transport is serial/// - `data`: Raw identity/status data provided by the manufacturer.
    fn report_server_id_response(
        &self,
        txn_id: u16,
        unit_id_slave_addr: UnitIdOrSlaveAddr,
        data: &[u8],
    );
}
