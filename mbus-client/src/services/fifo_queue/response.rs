//! # Modbus FIFO Queue Response Handling
//!
//! This module provides the logic for parsing and dispatching responses related to
//! Modbus Read FIFO Queue (Function Code 0x18).
//!
//! ## Responsibilities
//! - **Parsing**: Validates PDU structure, function codes, and byte counts for FIFO responses.
//! - **De-encapsulation**: Extracts the 16-bit register values from the Modbus PDU.
//! - **Dispatching**: Routes the parsed data to the application layer via the `FifoQueueResponse` trait.
//!
//! ## Architecture
//! - `ResponseParser`: Contains low-level logic to transform raw PDU bytes into a list of register values.
//! - `ClientServices` implementation: Orchestrates the high-level handling, converting the PDU
//!   into a `FifoQueue` model and triggering the application callback.

use crate::app::FifoQueueResponse;
use crate::services::fifo_queue::MAX_FIFO_QUEUE_COUNT_PER_PDU;
use crate::services::{ClientCommon, ClientServices, ExpectedResponse, fifo_queue};
use mbus_core::{
    data_unit::common::{ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::Transport,
};

pub(super) struct ResponseParser;

impl ResponseParser {
    /// Internal parser for Read FIFO Queue response PDUs (FC 0x18).
    pub(super) fn parse_read_fifo_queue_response(
        pdu: &Pdu,
    ) -> Result<([u16; MAX_FIFO_QUEUE_COUNT_PER_PDU], usize), MbusError> {
        if pdu.function_code() != FunctionCode::ReadFifoQueue {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data = pdu.data().as_slice();
        // PDU Data: FIFO Byte Count (2 bytes), FIFO Count (2 bytes), N * Register Value (2 bytes each)
        if data.len() < 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let fifo_byte_count = u16::from_be_bytes([data[0], data[1]]) as usize;
        // The total data length should be 2 (for the byte count field itself) + fifo_byte_count.
        if data.len() != 2 + fifo_byte_count {
            return Err(MbusError::InvalidAduLength);
        }

        let fifo_count = u16::from_be_bytes([data[2], data[3]]) as usize;

        // The byte count should be 2 bytes for the FIFO count field, plus 2 bytes for each register.
        if fifo_byte_count != 2 + fifo_count * 2 {
            return Err(MbusError::ParseError);
        }

        if fifo_count > MAX_FIFO_QUEUE_COUNT_PER_PDU {
            return Err(MbusError::BufferLenMissmatch);
        }

        let mut values = [0u16; MAX_FIFO_QUEUE_COUNT_PER_PDU];
        let mut index = 0;
        for chunk in data[4..].chunks_exact(2) {
            if index >= MAX_FIFO_QUEUE_COUNT_PER_PDU {
                return Err(MbusError::BufferLenMissmatch);
            }
            values[index] = u16::from_be_bytes([chunk[0], chunk[1]]);
            index += 1;
        }

        if index != fifo_count {
            return Err(MbusError::ParseError);
        }

        Ok((values, fifo_count))
    }
}

impl<TRANSPORT, APP, const N: usize> ClientServices<TRANSPORT, APP, N>
where
    TRANSPORT: Transport,
    APP: ClientCommon + FifoQueueResponse,
{
    /// Orchestrates the processing of a Read FIFO Queue response.
    ///
    /// This method decompiles the PDU, validates the content, and notifies the
    /// application layer of success or failure.
    pub(super) fn handle_read_fifo_queue_response(
        &mut self,
        ctx: &ExpectedResponse<TRANSPORT, APP, N>,
        message: &ModbusMessage,
    ) {
        let pdu = message.pdu();
        let function_code = pdu.function_code();
        let transaction_id = ctx.txn_id;
        let unit_id_or_slave_addr = message.unit_id_or_slave_addr();

        let register_rsp = match fifo_queue::service::ServiceDecompiler::handle_read_fifo_queue_rsp(
            function_code,
            pdu,
        ) {
            Ok(register_response) => register_response,
            Err(e) => {
                self.app
                    .request_failed(transaction_id, unit_id_or_slave_addr, e);
                return;
            }
        };

        self.app
            .read_fifo_queue_response(transaction_id, unit_id_or_slave_addr, &register_rsp);
    }
}
