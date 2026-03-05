use crate::{app::Coils, client::services::{fifo::FifoQueue, registers::Registers}, errors::MbusError};

pub trait RequestErrorNotifier {
    /// Handles a failed request by invoking the appropriate application callback with the error information.
    /// This method will be called when a Modbus request related to service request fails,
    /// allowing application developers to log the error,
    /// update their application state, or take other appropriate actions based on the error information.
    fn request_failed(&self, txn_id: u16, unit_id: u8, error: MbusError);
}

/// Trait defining the expected response handling for coil-related Modbus operations.
/// Implementors of this trait to deliver the responses to the application layer,
/// allowing application developers to process the coil data and update their application state accordingly.
pub trait CoilResponse {
    /// Handles a Read Coils response by invoking the appropriate application callback with the coil states.
    /// This method will be called when a Read Coils response is received,
    /// and application developers can use it to process the coil data and update their application state accordingly.
    fn read_coils_response(&self, txn_id: u16, unit_id: u8, coils: &Coils, quantity: u16);

    /// Handles a Write Single Coil response by invoking the appropriate application callback with the address and
    /// value of the coil that was written.
    /// This method will be called when a Write Single Coil response is received,
    fn read_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool);

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback with the starting address
    /// and quantity of the coils that were written.
    /// This method will be called when a Write Multiple Coils response is received,
    /// and application developers can use it to update their application state accordingly.
    fn write_single_coil_response(&self, txn_id: u16, unit_id: u8, address: u16, value: bool);

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback with the starting address
    /// and quantity of the coils that were written.
    fn write_multiple_coils_response(&self, txn_id: u16, unit_id: u8, address: u16, quantity: u16);
}

pub trait FifoQueueResponse {
    fn read_fifo_queue_response(&mut self, txn_id: u16, unit_id: u8, fifo_queue: &FifoQueue);
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
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `registers`: A `Registers` struct containing the values of the read input registers.
    fn read_input_register_response(&mut self, txn_id: u16, unit_id: u8, registers: &Registers);

    /// Handles a response for a `Read Single Input Register` (FC 0x04) request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `address`: The address of the register that was read.
    /// - `value`: The value of the read register.
    fn read_single_input_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Read Holding Registers` (FC 0x03) request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `registers`: A `Registers` struct containing the values of the read holding registers.
    fn read_holding_registers_response(&mut self, txn_id: u16, unit_id: u8, registers: &Registers);

    /// Handles a response for a `Write Single Register` (FC 0x06) request, confirming a successful write.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `address`: The address of the register that was written.
    /// - `value`: The value that was written to the register.
    fn write_single_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Write Multiple Registers` (FC 0x10) request, confirming a successful write.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `starting_address`: The starting address of the registers that were written.
    /// - `quantity`: The number of registers that were written.
    fn write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        starting_address: u16,
        quantity: u16,
    );

    /// Handles a response for a `Read/Write Multiple Registers` (FC 0x17) request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `registers`: A `Registers` struct containing the values of the registers that were read.
    fn read_write_multiple_registers_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        registers: &Registers,
    );

    /// Handles a response for a single register read request.
    ///
    /// This is a convenience callback for when only one register is requested.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `address`: The address of the register that was read.
    /// - `value`: The value of the read register.
    fn read_single_register_response(&mut self, txn_id: u16, unit_id: u8, address: u16, value: u16);

    /// Handles a response for a single holding register write request.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    /// - `address`: The address of the register that was written.
    /// - `value`: The value that was written to the register.
    fn read_single_holding_register_response(
        &mut self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
    );

    /// Handles a response for a `Mask Write Register` (FC 0x16) request, confirming a successful operation.
    ///
    /// # Parameters
    /// - `txn_id`: The transaction ID of the original request.
    /// - `unit_id`: The unit ID of the device that responded.
    fn mask_write_register_response(&mut self, txn_id: u16, unit_id: u8);
}
