//! # Modbus Discrete Input Response Handling
//!
//! This module provides the logic for parsing and dispatching responses related to
//! Modbus Discrete Inputs (Function Code 0x02).
//!
//! ## Responsibilities
//! - **Parsing**: Validates PDU structure, function codes, and byte counts for Read Discrete Input responses.
//! - **De-encapsulation**: Extracts bit-packed input states from the Modbus PDU.
//! - **Dispatching**: Routes the parsed data to the application layer via the `DiscreteInputResponse` trait.
//!
//! ## Architecture
//! - `ResponseParser`: Contains low-level logic to transform raw PDU bytes into `DiscreteInputs` models.
//! - `ClientServices` implementation: Orchestrates the high-level handling, distinguishing between
//!   single-input requests and multiple-input requests to trigger the appropriate application callback.

use crate::app::DiscreteInputResponse;
use crate::services::discrete_input::DiscreteInputs;
use crate::services::{ClientCommon, ClientServices, ExpectedResponse, discrete_input};

use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    models::discrete_input::MAX_DISCRETE_INPUT_BYTES,
    transport::Transport,
};

/// Internal parser for Discrete Input response PDUs.
pub(super) struct ResponseParser;

impl ResponseParser {
    /// Parses a Modbus PDU response for a Read Discrete Inputs (FC 0x02) request.
    pub(super) fn parse_read_discrete_inputs_response(
        pdu: &Pdu,
        from_address: u16,
        expected_quantity: u16,
    ) -> Result<DiscreteInputs, MbusError> {
        // Ensure the function code matches Read Discrete Inputs (0x02)
        if pdu.function_code() != FunctionCode::ReadDiscreteInputs {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data_slice = pdu.data().as_slice();
        // PDU must at least contain the Byte Count field
        if data_slice.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }

        let byte_count = data_slice[0] as usize;
        // Validate that the PDU data length matches the declared byte count (+1 for the count field itself)
        if byte_count + 1 != data_slice.len() {
            return Err(MbusError::InvalidByteCount);
        }

        let expected_byte_count = expected_quantity.div_ceil(8) as usize;
        // Validate that the server returned the correct number of bytes for the requested quantity
        if byte_count != expected_byte_count {
            return Err(MbusError::InvalidQuantity);
        }

        // Initialize a fixed-size array for bit-packed states to avoid dynamic allocation.
        let mut inputs = [0u8; MAX_DISCRETE_INPUT_BYTES];

        // Copy the PDU data payload (excluding the byte count field) into our local array.
        // The length is already validated against expected_byte_count above.
        inputs[..byte_count].copy_from_slice(&data_slice[1..1 + byte_count]);

        // Construct the DiscreteInputs model which provides helper methods for bit access.
        let discrete_inputs = DiscreteInputs::new(from_address, expected_quantity)?
            .with_values(&inputs, expected_quantity)?;

        Ok(discrete_inputs)
    }
}

impl<T, APP, const N: usize> ClientServices<T, APP, N>
where
    T: Transport,
    APP: ClientCommon + DiscreteInputResponse,
{
    /// Orchestrates the processing of a Read Discrete Inputs response.
    ///
    /// This method decompiles the PDU, validates the content against the original request
    /// stored in `ExpectedResponse`, and notifies the application layer of success or failure.
    pub(super) fn handle_read_discrete_inputs_response(
        &mut self,
        ctx: &ExpectedResponse<T, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let expected_quantity = ctx.operation_meta.quantity();
        let from_address = ctx.operation_meta.address();
        let function_code = pdu.function_code();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let discrete_inputs =
            match discrete_input::service::ServiceDecompiler::handle_read_discrete_inputs_response(
                function_code,
                pdu,
                from_address,
                expected_quantity,
            ) {
                Ok(discrete_input_response) => discrete_input_response,
                Err(e) => {
                    // Parsing or validation of the discrete input response failed. The response is dropped.
                    self.app
                        .request_failed(transaction_id, unit_id_or_slave_addr, e);
                    return;
                }
            };

        // Determine if this was a high-level "Single" request or a "Multiple" request
        if ctx.operation_meta.is_single() {
            // Query the exact address that was requested instead of address 0
            let value = match discrete_inputs.value(from_address) {
                Ok(v) => v,
                Err(_) => {
                    self.app.request_failed(
                        transaction_id,
                        unit_id_or_slave_addr,
                        MbusError::Unexpected,
                    );
                    return; // nothing to report, drop the response
                }
            };

            // Notify app of a single bit result
            self.app.read_single_discrete_input_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                value,
            );
        } else {
            // Notify app of the full collection of bits
            self.app.read_multiple_discrete_inputs_response(
                transaction_id,
                unit_id_or_slave_addr,
                &discrete_inputs,
            );
        }
    }
}
