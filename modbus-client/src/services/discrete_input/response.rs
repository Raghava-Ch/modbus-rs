use heapless::Vec;

use crate::app::DiscreteInputResponse;
use crate::services::discrete_input::DiscreteInputs;
use crate::services::{ClientCommon, ClientServices, ExpectedResponse, discrete_input};

use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::Transport,
};

pub(super) struct ResponseParser;

impl ResponseParser {
    /// Parses a Modbus PDU response for a Read Discrete Inputs (FC 0x02) request.
    pub(super) fn parse_read_discrete_inputs_response(
        pdu: &Pdu,
        from_address: u16,
        expected_quantity: u16,
    ) -> Result<DiscreteInputs, MbusError> {
        if pdu.function_code() != FunctionCode::ReadDiscreteInputs {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data_slice = pdu.data().as_slice();
        if data_slice.is_empty() {
            return Err(MbusError::InvalidDataLen);
        }

        let byte_count = data_slice[0] as usize;
        if byte_count + 1 != data_slice.len() {
            return Err(MbusError::InvalidByteCount);
        }

        let expected_byte_count = ((expected_quantity + 7) / 8) as usize;
        if byte_count != expected_byte_count {
            return Err(MbusError::InvalidQuantity);
        }
        let inputs =
            Vec::from_slice(&data_slice[1..]).map_err(|_| MbusError::BufferLenMissmatch)?;
        Ok(DiscreteInputs::new(from_address, expected_quantity, inputs))
    }
}

impl<T, APP, const N: usize> ClientServices<T, APP, N>
where
    T: Transport,
    APP: ClientCommon + DiscreteInputResponse,
{
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

            self.app.read_single_discrete_input_response(
                transaction_id,
                unit_id_or_slave_addr,
                from_address,
                value,
            );
        } else {
            self.app.read_multiple_discrete_inputs_response(
                transaction_id,
                unit_id_or_slave_addr,
                &discrete_inputs,
            );
        }
    }
}
