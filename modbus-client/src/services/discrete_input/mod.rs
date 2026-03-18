pub mod request;
pub mod response;

pub use mbus_core::models::discrete_input::*;

mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::services::discrete_input::{
        self, request::ReqPduCompiler, response::ResponseParser,
    };
    use mbus_core::{
        data_unit::common::Pdu,
        errors::MbusError,
        function_codes::public::FunctionCode,
        models::discrete_input::DiscreteInputs,
        transport::TransportType,
    };

    // --- Request Creation Tests ---

    #[test]
    fn test_read_discrete_inputs_request_valid() {
        let pdu = ReqPduCompiler::read_discrete_inputs_request(0x00C4, 0x0016).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadDiscreteInputs);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0xC4, 0x00, 0x16]);
    }

    #[test]
    fn test_read_discrete_inputs_request_min_max_quantity() {
        // Min quantity: 1
        let pdu = ReqPduCompiler::read_discrete_inputs_request(0, 1).unwrap();
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x01]);

        // Max quantity: 2000
        let pdu = ReqPduCompiler::read_discrete_inputs_request(0, 2000).unwrap();
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x07, 0xD0]);
    }

    #[test]
    fn test_read_discrete_inputs_request_invalid_quantity() {
        // Zero
        assert_eq!(
            ReqPduCompiler::read_discrete_inputs_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
        // Too large (2001)
        assert_eq!(
            ReqPduCompiler::read_discrete_inputs_request(0, 2001).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    // --- Response Parsing Tests ---

    #[test]
    fn test_parse_read_discrete_inputs_response_valid() {
        // Example from MODBUS Application Protocol Specification V1.1b3, section 6.2
        // Inputs 197-218 (22 inputs).
        // Data: 0xAC, 0xDB, 0x35
        let response_data = [0x03, 0xAC, 0xDB, 0x35]; // byte_count, data...
        let pdu = Pdu::new(
            FunctionCode::ReadDiscreteInputs,
            Vec::from_slice(&response_data).unwrap(),
            4,
        );
        let inputs = ResponseParser::parse_read_discrete_inputs_response(&pdu, 196, 22).unwrap();
        assert_eq!(inputs.values(), &[0xAC, 0xDB, 0x35]);
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_wrong_fc() {
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        assert_eq!(
            ResponseParser::parse_read_discrete_inputs_response(&pdu, 1, 1).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_empty_data() {
        let pdu = Pdu::new(FunctionCode::ReadDiscreteInputs, Vec::new(), 0);
        assert_eq!(
            ResponseParser::parse_read_discrete_inputs_response(&pdu, 1, 1).unwrap_err(),
            MbusError::InvalidDataLen
        );
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_byte_count_mismatch_pdu_len() {
        // Byte count says 2, but only 1 byte follows
        let data = [0x02, 0x00];
        let pdu = Pdu::new(
            FunctionCode::ReadDiscreteInputs,
            Vec::from_slice(&data).unwrap(),
            2,
        );
        assert_eq!(
            ResponseParser::parse_read_discrete_inputs_response(&pdu, 1, 16).unwrap_err(),
            MbusError::InvalidByteCount
        );
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_byte_count_mismatch_expected_quantity() {
        // Expected 16 inputs -> 2 bytes.
        // Received byte count 1.
        let data = [0x01, 0xFF];
        let pdu = Pdu::new(
            FunctionCode::ReadDiscreteInputs,
            Vec::from_slice(&data).unwrap(),
            2,
        );
        assert_eq!(
            ResponseParser::parse_read_discrete_inputs_response(&pdu, 2, 16).unwrap_err(),
            MbusError::InvalidQuantity
        );
    }

    // --- DiscreteInputs Struct Tests ---

    #[test]
    fn test_discrete_inputs_value_access() {
        // 22 inputs starting at 196 (0xC4).
        // Data: 0xAC (1010 1100), 0xDB (1101 1011), 0x35 (0011 0101)
        // Byte 0 (0xAC):
        //   Bit 0 (196): 0
        //   Bit 1 (197): 0
        //   Bit 2 (198): 1
        //   Bit 3 (199): 1
        //   Bit 4 (200): 0
        //   Bit 5 (201): 1
        //   Bit 6 (202): 0
        //   Bit 7 (203): 1
        let values = Vec::from_slice(&[0xAC, 0xDB, 0x35]).unwrap();
        let inputs = DiscreteInputs::new(196, 22, values);

        assert_eq!(inputs.value(196).unwrap(), false);
        assert_eq!(inputs.value(198).unwrap(), true);
        assert_eq!(inputs.value(203).unwrap(), true);

        // Boundary checks
        assert_eq!(inputs.value(195).unwrap_err(), MbusError::InvalidAddress); // Too low
        assert_eq!(
            inputs.value(196 + 22).unwrap_err(),
            MbusError::InvalidAddress
        ); // Too high
    }

    // --- Service Tests ---

    #[test]
    fn test_service_read_discrete_inputs_tcp() {
        let adu = discrete_input::service::ServiceBuilder::read_discrete_inputs(
            0x1234,
            1,
            0,
            10,
            TransportType::StdTcp,
        )
        .unwrap();

        // MBAP: 12 34 00 00 00 06 01
        // PDU: 02 00 00 00 0A
        let expected = [
            0x12, 0x34, 0x00, 0x00, 0x00, 0x06, 0x01, // MBAP
            0x02, 0x00, 0x00, 0x00, 0x0A, // PDU
        ];
        assert_eq!(adu.as_slice(), &expected);
    }

    #[test]
    fn test_service_handle_response() {
        let data = [0x01, 0x01]; // 1 byte count, value 1
        let pdu = Pdu::new(
            FunctionCode::ReadDiscreteInputs,
            Vec::from_slice(&data).unwrap(),
            2,
        );

        let result = discrete_input::response::ResponseParser::parse_read_discrete_inputs_response(
            &pdu, 0, 8,
        );

        assert!(result.is_ok());
        let inputs = result.unwrap();
        assert_eq!(inputs.quantity(), 8);
        assert_eq!(inputs.values().as_slice(), &[0x01]);
    }

    #[test]
    fn test_service_handle_response_wrong_fc() {
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = discrete_input::response::ResponseParser::parse_read_discrete_inputs_response(
            &pdu, 0, 8,
        );
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }
}
