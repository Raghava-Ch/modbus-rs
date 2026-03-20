use heapless::Vec;

use crate::services::coil::{Coils, request::ReqPduCompiler, response::ResponseParser};
use mbus_core::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu},
    errors::MbusError,
    function_codes::public::FunctionCode,
    transport::TransportType,
};

/// Service for handling Modbus coil operations, including creating request PDUs and parsing responses.
#[derive(Debug, Clone)]
pub struct ServiceBuilder;

/// Provides operations for reading and writing Modbus coils, as well as parsing responses for coil-related function codes.
impl ServiceBuilder {
    /// Sends a Read Coils request to a Modbus server and registers the expected response.
    ///
    /// # Arguments
    /// * `txn_id` - The transaction ID for the request.
    /// * `unit_id` - The unit ID (slave address) of the Modbus server.
    /// * `address` - The starting address of the first coil to read (0-65535).
    /// * `quantity` - The number of coils to read (1-2000).
    /// * `single_read` - Whether this is a single coil read or multiple coils read.
    ///
    pub fn read_coils(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::read_coils_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Single Coil request to a Modbus server and registers the expected response.
    ///
    /// # Arguments
    /// * `txn_id` - The transaction ID for the request.
    /// * `unit_id` - The unit ID (slave address) of the Modbus server.
    /// * `address` - The address of the coil to write (0-65535).
    /// * `value` - The state to write to the coil (`true` for ON, `false` for OFF).
    /// # Returns
    /// A `Result` containing the raw bytes of the Modbus ADU to be sent, or an `MbusError` if the request could not be created.
    pub fn write_single_coil(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: bool,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::write_single_coil_request(address, value)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Multiple Coils request to a Modbus server and registers the expected response.
    pub fn write_multiple_coils(
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[bool],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = ReqPduCompiler::write_multiple_coils_request(address, quantity, values)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Handles a Read Coils response by invoking the appropriate application callback.
    pub fn handle_read_coil_rsp(
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Coils, MbusError> {
        if pdu.function_code() != FunctionCode::ReadCoils {
            return Err(MbusError::InvalidFunctionCode); // Mismatch in function code
        }
        let coil_response =
            ResponseParser::handle_coil_response(pdu, expected_quantity, from_address)?;

        Ok(coil_response)
    }

    /// Handles a Read Coils response for a single coil read by invoking the appropriate application callback.
    pub fn handle_write_single_coil_rsp(
        function_code: FunctionCode,
        pdu: &Pdu,
        address: u16,
        value: bool,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteSingleCoil {
            return Err(MbusError::InvalidFunctionCode);
        }
        if ResponseParser::parse_write_single_coil_response(pdu, address, value).is_ok() {
            Ok(())
        } else {
            Err(MbusError::ParseError)
        }
    }

    /// Handles a Write Multiple Coils response by invoking the appropriate application callback.
    pub fn handle_write_multiple_coils_rsp(
        function_code: FunctionCode,
        pdu: &Pdu,
        address: u16,
        quantity: u16,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteMultipleCoils {
            return Err(MbusError::InvalidFunctionCode);
        }
        if ResponseParser::parse_write_multiple_coils_response(pdu, address, quantity).is_ok() {
            Ok(())
        } else {
            Err(MbusError::ParseError)
        }
    }
}
