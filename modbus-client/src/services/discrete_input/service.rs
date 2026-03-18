use heapless::Vec;

use crate::services::discrete_input::{request::ReqPduCompiler, response::ResponseParser};
use mbus_core::{
    data_unit::common::{self, Pdu, MAX_ADU_FRAME_LEN},
    errors::MbusError,
    function_codes::public::FunctionCode,
    models::discrete_input::DiscreteInputs,
    transport::TransportType,
};

/// Provides service operations for reading Modbus discrete inputs.
#[derive(Debug, Clone)]
pub struct ServiceBuilder;

impl ServiceBuilder {
    /// Sends a Read Discrete Inputs (FC 0x02) request.
    pub(crate) fn read_discrete_inputs(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_discrete_inputs_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }
}

pub(crate) struct ServiceDecompiler;

impl ServiceDecompiler {
    pub(crate) fn handle_read_discrete_inputs_response(
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<DiscreteInputs, MbusError> {
        if function_code != FunctionCode::ReadDiscreteInputs {
            return Err(MbusError::InvalidFunctionCode);
        }
        ResponseParser::parse_read_discrete_inputs_response(pdu, from_address, expected_quantity)
    }
}
