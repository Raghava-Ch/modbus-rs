//! # Modbus Coil Response Handling
//!
//! This module provides the logic for parsing and dispatching responses related to
//! Modbus Coils (Function Codes 0x01, 0x05, 0x0F).
//!
//! ## Responsibilities
//! - **Parsing**: Validates PDU structure, function codes, and byte counts for coil operations.
//! - **De-encapsulation**: Extracts bit-packed coil states or confirmation metadata from the Modbus PDU.
//! - **Dispatching**: Routes the parsed data to the application layer via the `CoilResponse` trait.
//!
//! ## Architecture
//! - `ResponseParser`: Contains low-level logic to transform raw PDU bytes into boolean collections or success signals.
//! - `ClientServices` implementation: Orchestrates the high-level handling, distinguishing between
//!   single-coil requests and multiple-coil requests to trigger the appropriate application callback.

use heapless::Vec;

use crate::{
    app::CoilResponse,
    services::ExpectedResponse,
    services::coil::{Coils, MAX_COIL_BYTES},
    services::{ClientCommon, ClientServices, coil},
};
use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::Transport,
};

/// # ResponseParser
///
/// A low-level utility for decoding Modbus Protocol Data Units (PDUs) specific to coil operations.
///
/// This struct provides stateless methods to validate function codes, verify byte counts,
/// and extract boolean states or confirmation metadata from raw byte buffers.
pub(super) struct ResponseParser;

impl ResponseParser {
    /// Parses a Modbus PDU response for a Write Multiple Coils (FC 0x0F) request.
    ///
    /// This function validates the response from a Modbus server for a Write Multiple Coils
    /// operation, ensuring the function code, starting address, and quantity match the request.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The starting address that was written in the request.
    /// * `expected_quantity` - The quantity of coils that was written in the request.
    ///
    /// # Returns
    /// `Ok(())` if the response is valid and matches the request, or an `MbusError` otherwise.
    pub(super) fn parse_write_multiple_coils_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteMultipleCoils {
            return Err(MbusError::ParseError);
        }

        let data_slice = pdu.data().as_slice();

        if data_slice.len() != 4 {
            // Address (2 bytes) + Quantity (2 bytes)
            return Err(MbusError::InvalidDataLen);
        }

        let response_address = u16::from_be_bytes([data_slice[0], data_slice[1]]);
        let response_quantity = u16::from_be_bytes([data_slice[2], data_slice[3]]);

        if response_address != expected_address {
            return Err(MbusError::InvalidAddress); // Mismatch in address or quantity
        }

        if response_quantity != expected_quantity {
            return Err(MbusError::InvalidQuantity); // Mismatch in address or quantity
        }

        Ok(())
    }

    /// Handles a Read Coils response by invoking the appropriate application callback.
    /// This function parses the PDU received from a Modbus server in response to a Read Coils request,
    /// extracting the coil states and returning a `Coils` struct that can be used by the application layer.
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_quantity` - The quantity of coils that was originally requested.
    /// * `from_address` - The starting address of the coils that were requested.
    /// # Returns
    /// An `Option<Coils>` containing the parsed coil states if the response is valid, or
    /// `None` if the response is malformed or does not match the expected quantity.
    pub(super) fn handle_coil_response(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Coils, MbusError> {
        let coil_response = Self::parse_read_coils_response(pdu, expected_quantity)?;
        let coils = Coils::new(from_address, expected_quantity)?
            .with_values(&coil_response, expected_quantity)?;

        Ok(coils)
    }

    /// Parses a Modbus PDU response for a Read Coils (FC 0x01) request for a single coil.
    ///
    /// This function interprets the PDU received from a Modbus server, extracting the
    /// boolean state of a single coil.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The address that was originally requested.
    ///
    /// # Returns
    /// A `Result` containing the boolean state of the coil, or an `MbusError` if
    /// the PDU is malformed or the data does not represent a single coil.
    /// Parses a Modbus PDU response for a Read Coils (FC 0x01) request.
    ///
    /// This function interprets the PDU received from a Modbus server in response
    /// to a Read Coils request, extracting the coil states.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_quantity` - The quantity of coils that was originally requested.
    ///
    /// # Returns
    /// A `Result` containing a `heapless::Vec<bool, 2000>` representing the coil states,
    /// or an `MbusError` if the PDU is malformed, contains an unexpected function code,
    /// or the data length does not match the expected quantity.
    pub(super) fn parse_read_coils_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u8, MAX_COIL_BYTES>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadCoils {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data_slice = pdu.data().as_slice();
        if data_slice.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }

        let byte_count = data_slice[0] as usize;
        // The PDU data should be: [byte_count, data_byte_1, ..., data_byte_N]
        // So, total length of data_slice should be 1 (for byte_count) + byte_count
        if byte_count + 1 != data_slice.len() {
            return Err(MbusError::InvalidByteCount);
        }

        // Calculate expected byte count: ceil(expected_quantity / 8)
        let expected_byte_count = expected_quantity.div_ceil(8) as usize;
        if byte_count != expected_byte_count {
            return Err(MbusError::InvalidQuantity); // Mismatch in expected byte count
        }

        let coils = Vec::from_slice(&data_slice[1..]).map_err(|_| MbusError::BufferLenMissmatch)?;
        Ok(coils)
    }

    /// Parses a Modbus PDU response for a Write Single Coil (FC 0x05) request.
    ///
    /// This function validates the response from a Modbus server for a Write Single Coil
    /// operation, ensuring the function code, address, and value match the request.
    ///
    /// # Arguments
    /// * `pdu` - The received `Pdu` from the Modbus server.
    /// * `expected_address` - The address that was written in the request.
    /// * `expected_value` - The value that was written in the request.
    ///
    /// # Returns
    /// `Ok(())` if the response is valid and matches the request, or an `MbusError` otherwise.
    pub(super) fn parse_write_single_coil_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_value: bool,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteSingleCoil {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data_slice = pdu.data().as_slice();

        if data_slice.len() != 4 {
            // Address (2 bytes) + Value (2 bytes)
            return Err(MbusError::InvalidDataLen);
        }

        let response_address = u16::from_be_bytes([data_slice[0], data_slice[1]]);
        let response_value = u16::from_be_bytes([data_slice[2], data_slice[3]]);

        if response_address != expected_address {
            return Err(MbusError::InvalidAddress); // Address mismatch
        }

        let expected_response_value = if expected_value { 0xFF00 } else { 0x0000 };
        if response_value != expected_response_value {
            return Err(MbusError::InvalidValue); // Value mismatch
        }

        Ok(())
    }
}

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + CoilResponse,
{
    /// Handles a Read Coils response by validating it against the expected response metadata and invoking the appropriate application callback.
    ///
    /// # Parameters
    /// - `mbap_header`: The MBAP header from the received message, used to extract transaction ID and unit ID for callbacks.
    /// - `function_code`: The function code from the PDU, used to determine how to parse the response.
    /// - `pdu`: The PDU from the received message, containing the actual response data to be parsed.
    /// - `expected_quantity`: The number of coils that were expected in the response, used for validation.
    /// - `from_address`: The starting address of the coils that were requested, used for validation.
    /// - `single_read`: A boolean indicating whether this was a single coil read request, which affects how the response is processed and which callback is invoked.
    ///
    /// This method uses the coil service to parse the response PDU and validate it against the expected quantity and address.
    /// If it's a single read, it extracts the single coil value and invokes the `read_single_coil_response` callback. If it's a multiple read, it invokes the
    /// `read_coils_response` callback with the full coil response. If parsing or validation fails at any point,
    /// it simply returns without invoking callbacks (as there's no valid data to report).
    pub(super) fn handle_read_coils_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let expected_quantity = ctx.operation_meta.quantity();
        let from_address = ctx.operation_meta.address();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let coil_rsp = match coil::service::ServiceBuilder::handle_read_coil_rsp(
            pdu,
            expected_quantity,
            from_address,
        ) {
            Ok(coil_response) => coil_response,
            Err(e) => {
                // Parsing or validation of the coil response failed. The response is dropped.
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
                return;
            }
        };
        if ctx.operation_meta.is_single() {
            // For single read, extract the value of the single coil; bail out if none.
            let coil_value = match coil_rsp.value(from_address) {
                Ok(v) => v,
                Err(_) => return, // Err(MbusError::ParseError), // nothing to report, drop the response
            }; // If no value is found for a single coil, the response is dropped. This should never happen in practical.

            self.app.read_single_coil_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                coil_value,
            );
        } else {
            self.app
                .read_coils_response(transaction_id, unit_id_or_slave_addr, &coil_rsp);
        }
    }

    /// Handles a Write Single Coil response by invoking the appropriate application callback.
    pub(super) fn handle_write_single_coil_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let function_code = pdu.function_code();
        let address = ctx.operation_meta.address();
        let value = ctx.operation_meta.value() != 0;
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        if coil::service::ServiceBuilder::handle_write_single_coil_rsp(
            function_code,
            pdu,
            address,
            value,
        )
        .is_ok()
        {
            // If successful
            self.app.write_single_coil_response(
                transaction_id,
                message.unit_id_or_slave_addr(),
                address,
                value,
            );
        } else {
            // If parsing or validation fails
            self.app
                .request_failed(transaction_id, unit_id_or_slave_addr, MbusError::ParseError);
        }
    }

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback.
    pub(super) fn handle_write_multiple_coils_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let function_code = message.pdu().function_code();
        let pdu = message.pdu();
        let txn_id = message.transaction_id();
        let unit_id = message.unit_id_or_slave_addr();
        let address = ctx.operation_meta.address();
        let quantity = ctx.operation_meta.quantity();
        if coil::service::ServiceBuilder::handle_write_multiple_coils_rsp(
            function_code,
            pdu,
            address,
            quantity,
        )
        .is_ok()
        {
            // If successful
            self.app
                .write_multiple_coils_response(txn_id, unit_id, address, quantity);
        } else {
            // If parsing or validation fails
            self.app
                .request_failed(txn_id, unit_id, MbusError::ParseError);
        }
    }
}
