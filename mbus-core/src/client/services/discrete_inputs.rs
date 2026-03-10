use crate::{
    data_unit::common::{AdditionalAddress, MAX_ADU_FRAME_LEN, MbapHeader, ModbusMessage, Pdu},
    errors::MbusError,
    function_codes::public::{FunctionCode, MAX_PDU_DATA_LEN},
    transport::TransportType,
};
use heapless::Vec;

/// Maximum number of discrete inputs that can be read in a single Modbus PDU (2000 inputs).
const MAX_DISCRETE_INPUTS_PER_PDU: usize = 2000;
/// Maximum number of bytes needed to represent the input states for 2000 inputs (250 bytes).
pub const MAX_DISCRETE_INPUT_BYTES: usize = (MAX_DISCRETE_INPUTS_PER_PDU + 7) / 8;

/// Represents the state of a block of discrete inputs read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct DiscreteInputs {
    /// The starting address of the first input in this block.
    from_address: u16,
    /// The number of inputs in this block.
    quantity: u16,
    /// The input states packed into bytes, where each bit represents an input (1 for ON, 0 for OFF).
    values: Vec<u8, MAX_DISCRETE_INPUT_BYTES>,
}

impl DiscreteInputs {
    /// Creates a new `DiscreteInputs` instance.
    pub fn new(
        from_address: u16,
        quantity: u16,
        values: Vec<u8, MAX_DISCRETE_INPUT_BYTES>,
    ) -> Self {
        Self {
            from_address,
            quantity,
            values,
        }
    }

    /// Returns the starting address.
    pub fn from_address(&self) -> u16 {
        self.from_address
    }

    /// Returns the quantity of inputs.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns the input values as bytes.
    pub fn values(&self) -> &Vec<u8, MAX_DISCRETE_INPUT_BYTES> {
        &self.values
    }

    /// Retrieves the boolean state of a specific input by its address.
    pub fn value(&self, address: u16) -> Result<bool, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let bit_index = (address - self.from_address) as usize;
        let byte_index = bit_index / 8;
        let bit_mask = 1u8 << (bit_index % 8);

        Ok(self.values[byte_index] & bit_mask != 0)
    }
}

#[derive(Debug, Clone)]
pub struct DiscreteInputService;

impl DiscreteInputService {
    pub fn new() -> Self {
        Self
    }

    pub fn read_discrete_inputs(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = DiscreteInputReqPdu::read_discrete_inputs_request(address, quantity)?;
        match transport_type {
            TransportType::StdTcp | TransportType::CustomTcp => {
                let pdu_bytes_len = pdu.to_bytes()?.len() as u16;
                let mbap_header = MbapHeader::new(txn_id, pdu_bytes_len + 1, unit_id);
                ModbusMessage::new(AdditionalAddress::MbapHeader(mbap_header), pdu).to_bytes()
            }
            TransportType::StdSerial(_slave_address, _serial_mode)
            | TransportType::CustomSerial(_slave_address, _serial_mode) => {
                todo!("Serial transport is not yet implemented for Read Discrete Inputs.")
            }
        }
    }

    pub fn handle_read_discrete_inputs_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<DiscreteInputs, MbusError> {
        if function_code != FunctionCode::ReadDiscreteInputs {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values =
            DiscreteInputReqPdu::parse_read_discrete_inputs_response(pdu, expected_quantity)?;
        Ok(DiscreteInputs::new(from_address, expected_quantity, values))
    }
}

/// Provides operations for reading Modbus discrete inputs.
pub struct DiscreteInputReqPdu {}

impl DiscreteInputReqPdu {
    /// Creates a Modbus PDU for a Read Discrete Inputs (FC 0x02) request.
    ///
    /// # Arguments
    /// * `address` - The starting address of the first input to read (0-65535).
    /// * `quantity` - The number of inputs to read (1-2000).
    ///
    /// # Returns
    /// A `Result` containing the constructed `Pdu` or an `MbusError` if the
    /// quantity is out of the valid Modbus range (1 to 2000).
    pub fn read_discrete_inputs_request(address: u16, quantity: u16) -> Result<Pdu, MbusError> {
        if !(1..=2000).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadDiscreteInputs, data_vec, 4))
    }

    /// Parses a Modbus PDU response for a Read Discrete Inputs (FC 0x02) request.
    pub fn parse_read_discrete_inputs_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u8, MAX_DISCRETE_INPUT_BYTES>, MbusError> {
        if pdu.function_code() != FunctionCode::ReadDiscreteInputs {
            return Err(MbusError::InvalidFunctionCode);
        }

        let data_slice = pdu.data().as_slice();
        if data_slice.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let byte_count = data_slice[0] as usize;
        if byte_count + 1 != data_slice.len() {
            return Err(MbusError::InvalidPduLength);
        }

        let expected_byte_count = ((expected_quantity + 7) / 8) as usize;
        if byte_count != expected_byte_count {
            return Err(MbusError::ParseError);
        }

        Vec::from_slice(&data_slice[1..]).map_err(|_| MbusError::BufferLenMissmatch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function_codes::public::FunctionCode;

    // --- Request Creation Tests ---

    #[test]
    fn test_read_discrete_inputs_request_valid() {
        let pdu = DiscreteInputReqPdu::read_discrete_inputs_request(0x00C4, 0x0016).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadDiscreteInputs);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0xC4, 0x00, 0x16]);
    }

    #[test]
    fn test_read_discrete_inputs_request_min_max_quantity() {
        // Min quantity: 1
        let pdu = DiscreteInputReqPdu::read_discrete_inputs_request(0, 1).unwrap();
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x01]);

        // Max quantity: 2000
        let pdu = DiscreteInputReqPdu::read_discrete_inputs_request(0, 2000).unwrap();
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x07, 0xD0]);
    }

    #[test]
    fn test_read_discrete_inputs_request_invalid_quantity() {
        // Zero
        assert_eq!(
            DiscreteInputReqPdu::read_discrete_inputs_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
        // Too large (2001)
        assert_eq!(
            DiscreteInputReqPdu::read_discrete_inputs_request(0, 2001).unwrap_err(),
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
        let inputs = DiscreteInputReqPdu::parse_read_discrete_inputs_response(&pdu, 22).unwrap();
        assert_eq!(inputs.as_slice(), &[0xAC, 0xDB, 0x35]);
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_wrong_fc() {
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        assert_eq!(
            DiscreteInputReqPdu::parse_read_discrete_inputs_response(&pdu, 1).unwrap_err(),
            MbusError::InvalidFunctionCode
        );
    }

    #[test]
    fn test_parse_read_discrete_inputs_response_empty_data() {
        let pdu = Pdu::new(FunctionCode::ReadDiscreteInputs, Vec::new(), 0);
        assert_eq!(
            DiscreteInputReqPdu::parse_read_discrete_inputs_response(&pdu, 1).unwrap_err(),
            MbusError::InvalidPduLength
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
            DiscreteInputReqPdu::parse_read_discrete_inputs_response(&pdu, 16).unwrap_err(),
            MbusError::InvalidPduLength
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
            DiscreteInputReqPdu::parse_read_discrete_inputs_response(&pdu, 16).unwrap_err(),
            MbusError::ParseError
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
        let service = DiscreteInputService::new();
        let adu = service
            .read_discrete_inputs(0x1234, 1, 0, 10, TransportType::StdTcp)
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
        let service = DiscreteInputService::new();
        let data = [0x01, 0x01]; // 1 byte count, value 1
        let pdu = Pdu::new(
            FunctionCode::ReadDiscreteInputs,
            Vec::from_slice(&data).unwrap(),
            2,
        );

        let result =
            service.handle_read_discrete_inputs_rsp(FunctionCode::ReadDiscreteInputs, &pdu, 8, 0);

        assert!(result.is_ok());
        let inputs = result.unwrap();
        assert_eq!(inputs.quantity(), 8);
        assert_eq!(inputs.values().as_slice(), &[0x01]);
    }

    #[test]
    fn test_service_handle_response_wrong_fc() {
        let service = DiscreteInputService::new();
        let pdu = Pdu::new(FunctionCode::ReadCoils, Vec::new(), 0);
        let result = service.handle_read_discrete_inputs_rsp(FunctionCode::ReadCoils, &pdu, 8, 0);
        assert_eq!(result.unwrap_err(), MbusError::InvalidFunctionCode);
    }
}
