use heapless::Vec;

use crate::services::fifo_queue::{FifoQueue, request::ReqPduCompiler, response::ResponseParser};
use mbus_core::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::TransportType,
};

/// Provides service operations for reading Modbus FIFO Queue.
#[derive(Debug, Clone)]
pub(crate) struct ServiceBuilder;

impl ServiceBuilder {
    /// Sends a Read FIFO Queue request.
    pub(crate) fn read_fifo_queue(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_fifo_queue_request(address)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct ServiceDecompiler;

impl ServiceDecompiler {
    /// Handles a Read FIFO Queue response.
    pub(crate) fn handle_read_fifo_queue_rsp(
        function_code: FunctionCode,
        pdu: &Pdu,
    ) -> Result<FifoQueue, MbusError> {
        if function_code != FunctionCode::ReadFifoQueue {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = ResponseParser::parse_read_fifo_queue_response(pdu)?;
        Ok(FifoQueue::new(0).with_values(values))
    }
}
