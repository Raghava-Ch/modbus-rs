use crate::{
    data_unit::common::{self, MAX_ADU_FRAME_LEN, Pdu},
    errors::MbusError,
    function_codes::public::{FunctionCode, MAX_PDU_DATA_LEN},
    transport::TransportType,
};

use heapless::Vec;

/// Maximum number of registers that can be read/written in a single Modbus PDU (125 registers).
pub const MAX_REGISTERS_PER_PDU: usize = 125;

/// Represents the state of a block of registers read from a Modbus server.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Registers {
    /// The starting address of the first register in this block.
    from_address: u16,
    /// The number of registers in this block.
    quantity: u16,
    /// The register values.
    values: Vec<u16, MAX_REGISTERS_PER_PDU>,
}

impl Registers {
    /// Creates a new `Registers` instance.
    pub fn new(from_address: u16, quantity: u16, values: Vec<u16, MAX_REGISTERS_PER_PDU>) -> Self {
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

    /// Returns the quantity of registers.
    pub fn quantity(&self) -> u16 {
        self.quantity
    }

    /// Returns the register values.
    pub fn values(&self) -> &Vec<u16, MAX_REGISTERS_PER_PDU> {
        &self.values
    }

    /// Retrieves the value of a specific register by its address.
    pub fn value(&self, address: u16) -> Result<u16, MbusError> {
        if address < self.from_address || address >= self.from_address + self.quantity {
            return Err(MbusError::InvalidAddress);
        }
        let index = (address - self.from_address) as usize;
        self.values
            .get(index)
            .copied()
            .ok_or(MbusError::InvalidAddress)
    }
}

#[derive(Debug, Clone)]
pub struct RegisterService;

impl RegisterService {
    /// Creates a new instance of `RegisterService`.
    pub fn new() -> Self {
        Self
    }

    /// Sends a Read Holding Registers request.
    pub fn read_holding_registers(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::read_holding_registers_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Read Input Registers request.
    pub fn read_input_registers(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::read_input_registers_request(address, quantity)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Single Register request.
    pub fn write_single_register(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        value: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::write_single_register_request(address, value)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Write Multiple Registers request.
    pub fn write_multiple_registers(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        quantity: u16,
        values: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::write_multiple_registers_request(address, quantity, values)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Read/Write Multiple Registers request.
    pub fn read_write_multiple_registers(
        &self,
        txn_id: u16,
        unit_id: u8,
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::read_write_multiple_registers_request(
            read_address,
            read_quantity,
            write_address,
            write_values,
        )?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    /// Sends a Mask Write Register request.
    pub fn mask_write_register(
        &self,
        txn_id: u16,
        unit_id: u8,
        address: u16,
        and_mask: u16,
        or_mask: u16,
        transport_type: TransportType,
    ) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let pdu = RegReqPdu::mask_write_register_request(address, and_mask, or_mask)?;
        common::compile_adu_frame(txn_id, unit_id, pdu, transport_type)
    }

    pub fn handle_write_single_register_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        address: u16,
        value: u16,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteSingleRegister {
            return Err(MbusError::InvalidFunctionCode);
        }
        RegReqPdu::parse_write_single_register_response(pdu, address, value)
    }

    /// Handles a Read Holding Registers response.
    pub fn handle_read_holding_register_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if function_code != FunctionCode::ReadHoldingRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = RegReqPdu::parse_read_holding_registers_response(pdu, expected_quantity)?;
        Ok(Registers::new(from_address, expected_quantity, values))
    }

    /// Handles a Read Input Registers response.
    pub fn handle_read_input_register_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if function_code != FunctionCode::ReadInputRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values = RegReqPdu::parse_read_input_registers_response(pdu, expected_quantity)?;
        Ok(Registers::new(from_address, expected_quantity, values))
    }

    /// Handles a Write Multiple Registers response.
    pub fn handle_write_multiple_registers_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::WriteMultipleRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        RegReqPdu::parse_write_multiple_registers_response(pdu, expected_address, expected_quantity)
    }

    /// Handles a Read/Write Multiple Registers response.
    pub fn handle_read_write_multiple_registers_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        expected_read_quantity: u16,
        from_address: u16,
    ) -> Result<Registers, MbusError> {
        if function_code != FunctionCode::ReadWriteMultipleRegisters {
            return Err(MbusError::InvalidFunctionCode);
        }
        let values =
            RegReqPdu::parse_read_write_multiple_registers_response(pdu, expected_read_quantity)?;
        Ok(Registers::new(from_address, expected_read_quantity, values))
    }

    /// Handles a Mask Write Register response.
    pub fn handle_mask_write_register_rsp(
        &self,
        function_code: FunctionCode,
        pdu: &Pdu,
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<(), MbusError> {
        if function_code != FunctionCode::MaskWriteRegister {
            return Err(MbusError::InvalidFunctionCode);
        }
        RegReqPdu::parse_mask_write_register_response(pdu, address, and_mask, or_mask)
    }
}

/// Provides operations for reading and writing Modbus registers.
pub struct RegReqPdu {}

/// The `RegReqPdu` struct provides methods to create Modbus PDUs for various register operations, including:
/// - Reading holding registers (FC 0x03)
/// - Reading input registers (FC 0x04)
/// - Writing a single register (FC 0x06)
/// - Writing multiple registers (FC 0x10)
/// - Read/Write multiple registers (FC 0x17)
/// - Mask write register (FC 0x16)
/// Each method validates the input parameters and constructs a PDU with the appropriate function code and data payload for the specified operation.
impl RegReqPdu {
    pub fn read_holding_registers_request(address: u16, quantity: u16) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadHoldingRegisters, data_vec, 4))
    }

    /// Creates a Modbus PDU for a Read Input Registers (FC 0x04) request.
    pub fn read_input_registers_request(address: u16, quantity: u16) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::ReadInputRegisters, data_vec, 4))
    }

    /// Creates a Modbus PDU for a Write Single Register (FC 0x06) request.
    pub fn write_single_register_request(address: u16, value: u16) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&value.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::WriteSingleRegister, data_vec, 4))
    }

    pub fn write_multiple_registers_request(
        address: u16,
        quantity: u16,
        values: &[u16],
    ) -> Result<Pdu, MbusError> {
        if !(1..=123).contains(&quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        if values.len() as u16 != quantity {
            return Err(MbusError::InvalidPduLength); // Mismatch between quantity and values length
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Add byte count (2 bytes per register)
        let byte_count = quantity * 2;
        data_vec
            .push(byte_count as u8)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for &value in values {
            data_vec
                .extend_from_slice(&value.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        let data_len = data_vec.len() as u8;
        Ok(Pdu::new(
            FunctionCode::WriteMultipleRegisters,
            data_vec,
            data_len,
        ))
    }

    pub fn read_write_multiple_registers_request(
        read_address: u16,
        read_quantity: u16,
        write_address: u16,
        write_values: &[u16],
    ) -> Result<Pdu, MbusError> {
        if !(1..=125).contains(&read_quantity) {
            return Err(MbusError::InvalidPduLength);
        }
        let write_quantity = write_values.len() as u16; // N
        if !(1..=121).contains(&write_quantity) {
            // Corrected max quantity for write_values in FC 0x17
            return Err(MbusError::InvalidPduLength);
        }

        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&read_address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&read_quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&write_address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&write_quantity.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        // Add byte count for write values
        let byte_count = write_quantity * 2;
        data_vec
            .push(byte_count as u8)
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        for &value in write_values {
            data_vec
                .extend_from_slice(&value.to_be_bytes())
                .map_err(|_| MbusError::BufferLenMissmatch)?;
        }

        Ok(Pdu::new(
            FunctionCode::ReadWriteMultipleRegisters,
            data_vec,
            9 + byte_count as u8, // 2 read_addr + 2 read_qty + 2 write_addr + 2 write_qty + 1 byte_count + N*2 bytes
        ))
    }

    pub fn mask_write_register_request(
        address: u16,
        and_mask: u16,
        or_mask: u16,
    ) -> Result<Pdu, MbusError> {
        let mut data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        data_vec
            .extend_from_slice(&address.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&and_mask.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;
        data_vec
            .extend_from_slice(&or_mask.to_be_bytes())
            .map_err(|_| MbusError::BufferLenMissmatch)?;

        Ok(Pdu::new(FunctionCode::MaskWriteRegister, data_vec, 6)) // Corrected: 2 addr + 2 and_mask + 2 or_mask
    }

    // --- Parsing Methods ---

    pub fn parse_read_holding_registers_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadHoldingRegisters,
            expected_quantity,
        )
    }

    pub fn parse_read_input_registers_response(
        pdu: &Pdu,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadInputRegisters,
            expected_quantity,
        )
    }

    fn parse_read_registers_response(
        pdu: &Pdu,
        expected_fc: FunctionCode,
        expected_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        if pdu.function_code() != expected_fc {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.is_empty() {
            return Err(MbusError::InvalidPduLength);
        }

        let byte_count = data[0] as usize;
        if data.len() != 1 + byte_count {
            return Err(MbusError::InvalidPduLength);
        }

        if byte_count != (expected_quantity * 2) as usize {
            return Err(MbusError::ParseError);
        }

        let mut values = Vec::new();
        for chunk in data[1..].chunks(2) {
            if chunk.len() == 2 {
                let val = u16::from_be_bytes([chunk[0], chunk[1]]);
                values
                    .push(val)
                    .map_err(|_| MbusError::BufferLenMissmatch)?;
            }
        }
        Ok(values)
    }

    pub fn parse_write_single_register_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_value: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteSingleRegister {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let value = u16::from_be_bytes([data[2], data[3]]);

        if address != expected_address || value != expected_value {
            return Err(MbusError::ParseError);
        }

        Ok(())
    }

    pub fn parse_write_multiple_registers_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_quantity: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::WriteMultipleRegisters {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 4 {
            return Err(MbusError::InvalidPduLength);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let quantity = u16::from_be_bytes([data[2], data[3]]);

        if address != expected_address || quantity != expected_quantity {
            return Err(MbusError::ParseError);
        }

        Ok(())
    }

    pub fn parse_read_write_multiple_registers_response(
        pdu: &Pdu,
        expected_read_quantity: u16,
    ) -> Result<Vec<u16, MAX_REGISTERS_PER_PDU>, MbusError> {
        Self::parse_read_registers_response(
            pdu,
            FunctionCode::ReadWriteMultipleRegisters,
            expected_read_quantity,
        )
    }

    pub fn parse_mask_write_register_response(
        pdu: &Pdu,
        expected_address: u16,
        expected_and_mask: u16,
        expected_or_mask: u16,
    ) -> Result<(), MbusError> {
        if pdu.function_code() != FunctionCode::MaskWriteRegister {
            return Err(MbusError::ParseError);
        }

        let data = pdu.data().as_slice();
        if data.len() != 6 {
            return Err(MbusError::InvalidPduLength);
        }

        let address = u16::from_be_bytes([data[0], data[1]]);
        let and_mask = u16::from_be_bytes([data[2], data[3]]);
        let or_mask = u16::from_be_bytes([data[4], data[5]]);

        if address != expected_address
            || and_mask != expected_and_mask
            || or_mask != expected_or_mask
        {
            return Err(MbusError::ParseError);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- Read Holding Registers (FC 0x03) ---

    /// Test case: `read_holding_registers_request` with valid parameters.
    #[test]
    fn test_read_holding_registers_request_valid() {
        let pdu = RegReqPdu::read_holding_registers_request(0x006B, 3).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x6B, 0x00, 0x03]);
    }

    /// Test case: `read_holding_registers_request` with invalid quantity (too low).
    #[test]
    fn test_read_holding_registers_invalid_quantity() {
        // Quantity 0 is invalid
        assert_eq!(
            RegReqPdu::read_holding_registers_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
        // Quantity 126 is invalid (max 125)
        assert_eq!(
            RegReqPdu::read_holding_registers_request(0, 126).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_holding_registers_request` with maximum allowed quantity.
    #[test]
    fn test_read_holding_registers_request_max_quantity() {
        let pdu = RegReqPdu::read_holding_registers_request(0x0000, 125).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadHoldingRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x7D]); // 125 = 0x7D
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Read Input Registers (FC 0x04) ---

    #[test]
    fn test_read_input_registers_request_valid() {
        let pdu = RegReqPdu::read_input_registers_request(0x0008, 1).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadInputRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x08, 0x00, 0x01]);
    }

    /// Test case: `read_input_registers_request` with invalid quantity (too low).
    #[test]
    fn test_read_input_registers_request_invalid_quantity_low() {
        assert_eq!(
            RegReqPdu::read_input_registers_request(0, 0).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_input_registers_request` with invalid quantity (too high).
    #[test]
    fn test_read_input_registers_request_invalid_quantity_high() {
        assert_eq!(
            RegReqPdu::read_input_registers_request(0, 126).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_input_registers_request` with maximum allowed quantity.
    #[test]
    fn test_read_input_registers_request_max_quantity() {
        let pdu = RegReqPdu::read_input_registers_request(0x0000, 125).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadInputRegisters);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0x00, 0x7D]); // 125 = 0x7D
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Write Single Register (FC 0x06) ---

    #[test]
    fn test_write_single_register_request_valid() {
        let pdu = RegReqPdu::write_single_register_request(0x0001, 0x0003).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteSingleRegister);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x01, 0x00, 0x03]);
        assert_eq!(pdu.data_len(), 4);
    }

    // --- Write Multiple Registers (FC 0x10) ---

    /// Test case: `write_multiple_registers_request` with valid data.
    #[test]
    fn test_write_multiple_registers_request_valid() {
        let quantity = 2;
        let values = [0x0001, 0x0002];
        let pdu = RegReqPdu::write_multiple_registers_request(0x0000, quantity, &values).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleRegisters);
        assert_eq!(
            pdu.data().as_slice(),
            &[0x00, 0x00, 0x00, 0x02, 0x04, 0x00, 0x01, 0x00, 0x02]
        );
        assert_eq!(pdu.data_len(), 9); // 2 addr + 2 qty + 1 byte_count + 4 data
    }

    /// Test case: `write_multiple_registers_request` with invalid quantity (too low).
    #[test]
    fn test_write_multiple_registers_request_invalid_quantity_low() {
        let values: [u16; 0] = [];
        assert_eq!(
            RegReqPdu::write_multiple_registers_request(0x0000, 0, &values).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `write_multiple_registers_request` with invalid quantity (too high).
    #[test]
    fn test_write_multiple_registers_request_invalid_quantity_high() {
        let values = [0x0000; 124]; // Max is 123
        assert_eq!(
            RegReqPdu::write_multiple_registers_request(0x0000, 124, &values).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `write_multiple_registers_request` returns an error for quantity-values mismatch.
    #[test]
    fn test_write_multiple_registers_request_quantity_values_mismatch() {
        let values = [0x1234, 0x5678];
        let result = RegReqPdu::write_multiple_registers_request(0x0001, 3, &values); // Quantity 3, but only 2 values
        assert_eq!(result.unwrap_err(), MbusError::InvalidPduLength);
    }

    /// Test case: `write_multiple_registers_request` with maximum allowed quantity.
    #[test]
    fn test_write_multiple_registers_request_max_quantity() {
        let values = [0x0000; 123]; // Max is 123
        let pdu = RegReqPdu::write_multiple_registers_request(0x0000, 123, &values).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::WriteMultipleRegisters);
        assert_eq!(pdu.data_len(), 5 + (123 * 2)); // 5 + 246 = 251
    }

    // --- Read/Write Multiple Registers (FC 0x17) ---

    /// Test case: `read_write_multiple_registers_request` with valid data.
    #[test]
    fn test_read_write_multiple_registers_request_valid() {
        let write_values = [0x0001, 0x0002];
        let pdu =
            RegReqPdu::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::ReadWriteMultipleRegisters
        );
        assert_eq!(
            pdu.data().as_slice(),
            &[
                0x00, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x01, 0x00, 0x02
            ]
        );
        assert_eq!(pdu.data_len(), 13); // 2 read_addr + 2 read_qty + 2 write_addr + 2 write_qty + 1 byte_count + 4 write_values
    }

    /// Test case: `read_write_multiple_registers_request` with invalid read quantity (too low).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_read_quantity_low() {
        let write_values = [0x0001];
        assert_eq!(
            RegReqPdu::read_write_multiple_registers_request(0x0000, 0, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid read quantity (too high).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_read_quantity_high() {
        let write_values = [0x0001];
        assert_eq!(
            RegReqPdu::read_write_multiple_registers_request(0x0000, 126, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid write quantity (too low).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_write_quantity_low() {
        let write_values: [u16; 0] = [];
        assert_eq!(
            RegReqPdu::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with invalid write quantity (too high).
    #[test]
    fn test_read_write_multiple_registers_request_invalid_write_quantity_high() {
        let write_values = [0x0000; 122]; // Max is 121
        assert_eq!(
            RegReqPdu::read_write_multiple_registers_request(0x0000, 1, 0x0001, &write_values)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `read_write_multiple_registers_request` with maximum allowed read and write quantities.
    #[test]
    fn test_read_write_multiple_registers_request_max_quantities() {
        let write_values = [0x0000; 121]; // Max write quantity
        let pdu =
            RegReqPdu::read_write_multiple_registers_request(0x0000, 125, 0x0001, &write_values)
                .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::ReadWriteMultipleRegisters
        );
        assert_eq!(pdu.data_len(), 9 + (121 * 2)); // 9 + 242 = 251
    }

    // --- Mask Write Register (FC 0x16) ---

    /// Test case: `mask_write_register_request` with valid data.
    #[test]
    fn test_mask_write_register_request_valid() {
        let pdu = RegReqPdu::mask_write_register_request(0x0004, 0xF002, 0x0025).unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::MaskWriteRegister);
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x04, 0xF0, 0x02, 0x00, 0x25]);
        assert_eq!(pdu.data_len(), 6);
    }

    // --- Parse Read/Write Multiple Registers Response Tests ---

    /// Test case: `parse_read_write_multiple_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_read_write_multiple_registers_response_valid() {
        // Response for reading 2 registers: FC(0x17), Byte Count(0x04), Data(0x1234, 0x5678)
        let response_bytes = [0x17, 0x04, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers = RegReqPdu::parse_read_write_multiple_registers_response(&pdu, 2).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_write_multiple_registers_response_wrong_fc() {
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for empty data.
    #[test]
    fn test_parse_read_write_multiple_registers_response_empty_data() {
        let pdu = Pdu::new(FunctionCode::ReadWriteMultipleRegisters, Vec::new(), 0);
        assert_eq!(
            RegReqPdu::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_write_multiple_registers_response_byte_count_mismatch() {
        let response_bytes = [0x17, 0x02, 0x12, 0x34, 0x56, 0x78]; // Byte count 2, but 4 data bytes
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_write_multiple_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_read_write_multiple_registers_response` returns an error for expected quantity mismatch.
    #[test]
    fn test_parse_read_write_multiple_registers_response_expected_quantity_mismatch() {
        let response_bytes = [0x17, 0x04, 0x12, 0x34, 0x56, 0x78]; // 2 registers in response
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_write_multiple_registers_response(&pdu, 3).unwrap_err(),
            MbusError::ParseError
        ); // Expected 3, got 2
    }

    // --- Parse Mask Write Register Response Tests ---

    /// Test case: `parse_mask_write_register_response` successfully parses a valid response.
    #[test]
    fn test_parse_mask_write_register_response_valid() {
        // Response: FC(0x16), Address(0x0004), AND Mask(0xF002), OR Mask(0x0025)
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x25];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025).is_ok()
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for wrong function code.
    #[test]
    fn test_parse_mask_write_register_response_wrong_fc() {
        let response_bytes = [0x06, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x25]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for invalid PDU length.
    #[test]
    fn test_parse_mask_write_register_response_invalid_len() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02]; // Too short
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for address mismatch.
    #[test]
    fn test_parse_mask_write_register_response_address_mismatch() {
        let response_bytes = [0x16, 0x00, 0x05, 0xF0, 0x02, 0x00, 0x25]; // Address mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for AND mask mismatch.
    #[test]
    fn test_parse_mask_write_register_response_and_mask_mismatch() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x01, 0x00, 0x25]; // AND mask mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_mask_write_register_response` returns an error for OR mask mismatch.
    #[test]
    fn test_parse_mask_write_register_response_or_mask_mismatch() {
        let response_bytes = [0x16, 0x00, 0x04, 0xF0, 0x02, 0x00, 0x26]; // OR mask mismatch
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_mask_write_register_response(&pdu, 0x0004, 0xF002, 0x0025)
                .unwrap_err(),
            MbusError::ParseError
        );
    }

    // --- Parse Read Holding Registers Response Tests ---

    /// Test case: `parse_read_holding_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_read_holding_registers_response_valid() {
        // Response for reading 2 registers: FC(0x03), Byte Count(0x04), Data(0x1234, 0x5678)
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        let registers = RegReqPdu::parse_read_holding_registers_response(&pdu, 2).unwrap();
        assert_eq!(registers.as_slice(), &[0x1234, 0x5678]);
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for wrong function code.
    #[test]
    fn test_parse_read_holding_registers_response_wrong_fc() {
        let response_bytes = [0x04, 0x04, 0x12, 0x34, 0x56, 0x78]; // Wrong FC
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_holding_registers_response(&pdu, 2).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for byte count mismatch.
    #[test]
    fn test_parse_read_holding_registers_response_byte_count_mismatch() {
        let response_bytes = [0x03, 0x03, 0x12, 0x34, 0x56, 0x78]; // Byte count 3, but 4 data bytes
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_holding_registers_response(&pdu, 2).unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    /// Test case: `parse_read_holding_registers_response` returns an error for expected quantity mismatch.
    #[test]
    fn test_parse_read_holding_registers_response_expected_quantity_mismatch() {
        let response_bytes = [0x03, 0x04, 0x12, 0x34, 0x56, 0x78]; // 2 registers in response
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_read_holding_registers_response(&pdu, 3).unwrap_err(),
            MbusError::ParseError
        ); // Expected 3, got 2
    }

    // --- Parse Write Single Register Response Tests ---

    /// Test case: `parse_write_single_register_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_single_register_response_valid() {
        let response_bytes = [0x06, 0x00, 0x01, 0x12, 0x34]; // FC, Address, Value
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(RegReqPdu::parse_write_single_register_response(&pdu, 0x0001, 0x1234).is_ok());
    }

    /// Test case: `parse_write_single_register_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_single_register_response_address_mismatch() {
        let response_bytes = [0x06, 0x00, 0x02, 0x12, 0x34];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_write_single_register_response(&pdu, 0x0001, 0x1234).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_write_single_register_response` returns an error for value mismatch.
    #[test]
    fn test_parse_write_single_register_response_value_mismatch() {
        let response_bytes = [0x06, 0x00, 0x01, 0x56, 0x78];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_write_single_register_response(&pdu, 0x0001, 0x1234).unwrap_err(),
            MbusError::ParseError
        );
    }

    // --- Parse Write Multiple Registers Response Tests ---

    /// Test case: `parse_write_multiple_registers_response` successfully parses a valid response.
    #[test]
    fn test_parse_write_multiple_registers_response_valid() {
        let response_bytes = [0x10, 0x00, 0x01, 0x00, 0x02]; // FC, Address, Quantity
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert!(RegReqPdu::parse_write_multiple_registers_response(&pdu, 0x0001, 2).is_ok());
    }

    /// Test case: `parse_write_multiple_registers_response` returns an error for address mismatch.
    #[test]
    fn test_parse_write_multiple_registers_response_address_mismatch() {
        let response_bytes = [0x10, 0x00, 0x02, 0x00, 0x02];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_write_multiple_registers_response(&pdu, 0x0001, 2).unwrap_err(),
            MbusError::ParseError
        );
    }

    /// Test case: `parse_write_multiple_registers_response` returns an error for quantity mismatch.
    #[test]
    fn test_parse_write_multiple_registers_response_quantity_mismatch() {
        let response_bytes = [0x10, 0x00, 0x01, 0x00, 0x03];
        let pdu = Pdu::from_bytes(&response_bytes).unwrap();
        assert_eq!(
            RegReqPdu::parse_write_multiple_registers_response(&pdu, 0x0001, 2).unwrap_err(),
            MbusError::ParseError
        );
    }

    // --- Registers struct tests ---

    /// Test case: `Registers::value` returns the correct value for a valid address.
    #[test]
    fn test_registers_value_valid() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        values_vec.push(0x5678).unwrap();
        let registers = Registers::new(0x0000, 2, values_vec);

        assert_eq!(registers.value(0x0000).unwrap(), 0x1234);
        assert_eq!(registers.value(0x0001).unwrap(), 0x5678);
    }

    /// Test case: `Registers::value` returns an error for an address below the range.
    #[test]
    fn test_registers_value_invalid_address_low() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        let registers = Registers::new(0x0001, 1, values_vec);

        assert_eq!(
            registers.value(0x0000).unwrap_err(),
            MbusError::InvalidAddress
        );
    }

    /// Test case: `Registers::value` returns an error for an address above the range.
    #[test]
    fn test_registers_value_invalid_address_high() {
        let mut values_vec = Vec::new();
        values_vec.push(0x1234).unwrap();
        let registers = Registers::new(0x0000, 1, values_vec);

        assert_eq!(
            registers.value(0x0001).unwrap_err(),
            MbusError::InvalidAddress
        );
    }
}
