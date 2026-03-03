use crate::data_unit::common::{AdditionalAddress, MbapHeader, ModbusMessage, Pdu, MAX_PDU_DATA_LEN};
use crate::errors::MbusError;
use crate::function_codes::public::FunctionCode;
use heapless::Vec; // Ensure Vec is imported

/// Modbus TCP Message Structure
#[derive(Debug, Clone)]
pub struct ModbusTcpMessage {
    modbus_message: ModbusMessage,
}

impl ModbusTcpMessage {
    pub fn new(mbap: MbapHeader, pdu: Pdu) -> Self {
        // Construct a ModbusTcpMessage from the given MBAP header and PDU.
        // This is a placeholder implementation; actual construction would involve
        // combining the MBAP header and PDU into a complete message format.
        ModbusTcpMessage {
            modbus_message: ModbusMessage {
                additional_address: AdditionalAddress::MbapHeader(mbap),
                pdu,
            },
        }
    }

    /// Converts the Modbus TCP message into its transferable Application Data Unit (ADU) byte representation.
    ///
    /// This method constructs the MBAP header and serializes the PDU into a single
    /// byte vector suitable for transmission over a TCP socket.
    ///
    /// # Returns
    /// `Ok(Vec<u8, 260>)` containing the complete Modbus TCP ADU, or an `MbusError` if
    /// the ADU cannot be constructed (e.g., due to buffer overflow).
    pub fn to_adu_bytes(&self) -> Result<Vec<u8, 260>, crate::errors::MbusError> {
        // SAFETY: We assume `additional_address` is correctly populated with `mbap_header` for ModbusTcpMessage.
        let mbap = if let AdditionalAddress::MbapHeader(mbap_header) =
            self.modbus_message.additional_address
        {
            mbap_header
        } else {
            return Err(crate::errors::MbusError::Unexpected);
        };
        let pdu_bytes = self.modbus_message.pdu.to_bytes()?;

        let pdu_len = pdu_bytes.len() as u16;
        let length_field = pdu_len + 1; // PDU length + 1 byte for Unit Identifier

        let mut adu = Vec::new(); // Capacity is 260 (7 bytes MBAP + 253 bytes PDU)

        // Transaction Identifier (2 bytes)
        adu.extend_from_slice(&mbap.transaction_id.to_be_bytes())
            .map_err(|_| crate::errors::MbusError::Unexpected)?;
        // Protocol Identifier (2 bytes)
        adu.extend_from_slice(&mbap.protocol_id.to_be_bytes())
            .map_err(|_| crate::errors::MbusError::Unexpected)?;
        // Length (2 bytes: Unit Identifier + PDU length)
        adu.extend_from_slice(&length_field.to_be_bytes())
            .map_err(|_| crate::errors::MbusError::Unexpected)?;
        // Unit Identifier (1 byte)
        adu.push(mbap.unit_id)
            .map_err(|_| crate::errors::MbusError::Unexpected)?;
        // Modbus PDU
        adu.extend_from_slice(&pdu_bytes)
            .map_err(|_| crate::errors::MbusError::Unexpected)?;

        Ok(adu)
    }

    /// Creates a `ModbusTcpMessage` from its byte representation (ADU).
    ///
    /// This method parses the MBAP header and the PDU from the given byte slice.
    ///
    /// # Arguments
    /// * `bytes` - A byte slice containing the complete Modbus TCP ADU.
    ///
    /// # Returns
    /// `Ok(ModbusTcpMessage)` if the bytes represent a valid ADU, or an `MbusError` otherwise.
    pub fn from_adu_bytes(bytes: &[u8]) -> Result<Self, MbusError> {
        // Minimum ADU length: 7 bytes MBAP header + 1 byte Function Code = 8 bytes
        if bytes.len() < 8 {
            return Err(MbusError::InvalidPduLength); // Reusing for general invalid length
        }

        // Parse MBAP Header
        let transaction_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let protocol_id = u16::from_be_bytes([bytes[2], bytes[3]]);
        let length = u16::from_be_bytes([bytes[4], bytes[5]]);
        let unit_id = bytes[6];

        // Validate Protocol Identifier
        if protocol_id != 0x0000 {
            return Err(MbusError::ParseError); // Invalid protocol ID
        }

        // Validate Length field
        // The length field specifies the number of following bytes, including the Unit ID and PDU.
        // So, actual_pdu_and_unit_id_len = bytes.len() - 6 (MBAP header without length field)
        // And length field value should be actual_pdu_and_unit_id_len
        let expected_total_len_from_header = length as usize + 6; // 6 bytes for TID, PID, Length field itself
        if bytes.len() != expected_total_len_from_header {
            return Err(MbusError::InvalidPduLength);
        }

        // The PDU starts after the MBAP header (7 bytes)
        let pdu_bytes_slice = &bytes[7..];

        // Parse PDU using the existing Pdu::from_bytes method
        let pdu = Pdu::from_bytes(pdu_bytes_slice)?;

        let mbap_header = MbapHeader {
            transaction_id,
            protocol_id,
            length,
            unit_id,
        };

        Ok(ModbusTcpMessage::new(mbap_header, pdu))
    }

    /// Returns a reference to the MBAP header of the Modbus TCP message.
    ///
    /// This method assumes that the `additional_address` field of the underlying `ModbusMessage`
    /// is of type `AdditionalAddress::MbapHeader`. If this is not the case, it will panic, as this should never happen for a properly constructed `ModbusTcpMessage`.
    ///
    /// # Returns
    /// A reference to the MBAP header.
    pub fn mbap_header(&self) -> &MbapHeader {
        if let AdditionalAddress::MbapHeader(mbap_header) = &self.modbus_message.additional_address
        {
            mbap_header
        } else {
            panic!("Expected MbapHeader: This should never happen, \nApplication developer error if this occurs");
        }
    }

    /// Returns a reference to the PDU of the Modbus TCP message.
    /// 
    /// # Returns
    /// A reference to the data payload of the PDU.
    pub fn data(&self) -> &Vec<u8, MAX_PDU_DATA_LEN> {
        self.modbus_message.data()
    }

    /// Returns the function code of the Modbus TCP message.
    ///
    /// # Returns
    /// The function code of the Modbus TCP message.
    pub fn pdu(&self) -> &Pdu {
        &self.modbus_message.pdu
    }

    /// Returns the function code of the Modbus TCP message.
    ///
    /// # Returns
    /// The function code of the Modbus TCP message.
    pub fn function_code(&self) -> FunctionCode {
        self.modbus_message.function_code()
    }

    /// Returns the length of the data payload in the PDU.
    ///
    /// # Returns
    /// The length of the data payload in the PDU.
    pub fn data_len(&self) -> u8 {
        self.modbus_message.data_len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;
    use crate::function_codes::public::FunctionCode;
    use heapless::Vec;

    // --- Tests for ModbusTcpMessage::to_adu_bytes ---

    /// Test case: `ModbusTcpMessage::to_adu_bytes` with a valid message.
    ///
    /// This tests the correct serialization of a Modbus TCP ADU.
    /// The PDU contains a function code and some data.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_to_adu_bytes_valid() {
        let mbap_header = MbapHeader {
            transaction_id: 0x1234,
            protocol_id: 0x0000,
            length: 0x0005, // This will be recalculated by to_adu_bytes, but set for consistency
            unit_id: 0x01,
        };

        let mut pdu_data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        pdu_data_vec.push(0x00).unwrap(); // Example data bytes
        pdu_data_vec.push(0x00).unwrap();
        pdu_data_vec.push(0x00).unwrap();

        let pdu = Pdu::new(
            FunctionCode::ReadHoldingRegisters,
            pdu_data_vec,
            3, // 3 data bytes
        );

        let tcp_message = ModbusTcpMessage::new(mbap_header, pdu);
        let adu_bytes = tcp_message.to_adu_bytes().expect("Failed to serialize ADU");

        #[rustfmt::skip]
        let expected_adu: [u8; 11] = [
            0x12, 0x34, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x05, // Length (PDU length (FC + Data) + 1 byte Unit ID = (1 + 3) + 1 = 5)
            0x01,       // Unit ID
            0x03,       // Function Code (Read Holding Registers)
            0x00, 0x00, 0x00, // Data
        ];

        assert_eq!(adu_bytes.as_slice(), &expected_adu);
    }

    /// Test case: `ModbusTcpMessage::to_adu_bytes` with maximum PDU data length.
    ///
    /// This tests serialization when the PDU contains the maximum allowed 252 data bytes.
    /// The total ADU size should be 7 (MBAP) + 1 (FC) + 252 (Data) = 260 bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size), Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_to_adu_bytes_max_pdu_length() {
        let mbap_header = MbapHeader {
            transaction_id: 0x0001,
            protocol_id: 0x0000,
            length: 0x00FF, // Placeholder, will be recalculated
            unit_id: 0xFF,
        };

        let mut pdu_data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        for i in 0..MAX_PDU_DATA_LEN {
            pdu_data_vec.push(i as u8).unwrap();
        }
        let pdu_data_slice = pdu_data_vec.as_slice();

        let pdu = Pdu::new(
            FunctionCode::WriteMultipleRegisters, // Example FC
            pdu_data_vec.clone(),
            252, // Max data bytes
        );

        let tcp_message = ModbusTcpMessage::new(mbap_header, pdu);
        let adu_bytes = tcp_message
            .to_adu_bytes()
            .expect("Failed to serialize max ADU");

        assert_eq!(adu_bytes.len(), 260); // 7 MBAP + 1 FC + 252 Data

        // Verify MBAP header
        // Transaction ID
        assert_eq!(&adu_bytes[0..2], &[0x00, 0x01]);
        // Protocol ID
        assert_eq!(&adu_bytes[2..4], &[0x00, 0x00]);
        // Length (PDU length (FC + Data) + 1 byte Unit ID = (1 + 252) + 1 = 254 = 0x00FE)
        // Length (0x00FE = 254)
        assert_eq!(&adu_bytes[4..6], &[0x00, 0xFE]);
        assert_eq!(adu_bytes[6], 0xFF); // Unit ID

        // Verify PDU
        assert_eq!(adu_bytes[7], FunctionCode::WriteMultipleRegisters as u8); // FC
        assert_eq!(&adu_bytes[8..260], pdu_data_slice); // Data
    }

    /// Test case: `ModbusTcpMessage::to_adu_bytes` with a PDU containing only a function code (no data).
    ///
    /// This tests serialization for requests like Report Server ID.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.13 (Report Server ID).
    #[test]
    fn test_modbus_tcp_message_to_adu_bytes_no_pdu_data() {
        let mbap_header = MbapHeader {
            transaction_id: 0x0002,
            protocol_id: 0x0000,
            length: 0x0002, // Placeholder
            unit_id: 0x0A,
        };

        let pdu = Pdu::new(
            FunctionCode::ReportServerId,
            Vec::new(), // No actual data used
            0,                       // 0 data bytes
        );

        let tcp_message = ModbusTcpMessage::new(mbap_header, pdu);
        let adu_bytes = tcp_message
            .to_adu_bytes()
            .expect("Failed to serialize ADU with no data");

        #[rustfmt::skip]
        let expected_adu: [u8; 8] = [
            0x00, 0x02, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x02, // Length (1 byte Unit ID + 1 byte FC = 2 bytes)
            0x0A,       // Unit ID
            0x11,       // Function Code (Report Server ID)
        ];

        assert_eq!(adu_bytes.as_slice(), &expected_adu);
    }

    // --- Tests for ModbusTcpMessage::from_adu_bytes ---

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with a valid ADU.
    ///
    /// This tests the correct deserialization of a Modbus TCP ADU.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_valid() {
        #[rustfmt::skip]
        let adu_bytes: [u8; 11] = [
            0x12, 0x34, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x05, // Length (1 byte Unit ID + 1 byte FC + 3 bytes Data)
            0x01,       // Unit ID
            0x03,       // Function Code (Read Holding Registers)
            0x00, 0x00, 0x00, // Data
        ];

        let tcp_message =
            ModbusTcpMessage::from_adu_bytes(&adu_bytes).expect("Failed to deserialize ADU");

        if let AdditionalAddress::MbapHeader(mbap_header) =
            tcp_message.modbus_message.additional_address
        {
            assert_eq!(mbap_header.transaction_id, 0x1234);
            assert_eq!(mbap_header.protocol_id, 0x0000);
            assert_eq!(mbap_header.length, 0x0005);
            assert_eq!(mbap_header.unit_id, 0x01);
        } else {
            panic!("Expected MbapHeader");
        }

        assert_eq!(
            tcp_message.modbus_message.function_code(),
            FunctionCode::ReadHoldingRegisters
        );
        assert_eq!(tcp_message.modbus_message.data_len(), 3);
        assert_eq!(tcp_message.modbus_message.data().as_slice(), &[0x00, 0x00, 0x00]);

    }

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with maximum PDU data length.
    ///
    /// This tests deserialization when the ADU contains a PDU with the maximum allowed 252 data bytes.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.1 (PDU Size), Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_max_pdu_length() {
        let mut adu_bytes_vec: Vec<u8, 260> = Vec::new();
        #[rustfmt::skip]
        adu_bytes_vec.extend_from_slice(&[
            0x00, 0x01, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0xFE, // Length (1 Unit ID + 1 FC + 252 Data = 254 = 0x00FE)
            0xFF,       // Unit ID
            FunctionCode::WriteMultipleRegisters as u8, // FC
        ]).expect("Failed to extend ADU bytes");

        let mut pdu_data_expected_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        for i in 0..MAX_PDU_DATA_LEN {
            pdu_data_expected_vec.push(i as u8).unwrap();
            adu_bytes_vec
                .push(i as u8)
                .expect("Failed to push data byte");
        }
        let pdu_data_expected_slice = pdu_data_expected_vec.as_slice();

        let tcp_message = ModbusTcpMessage::from_adu_bytes(&adu_bytes_vec)
            .expect("Failed to deserialize max ADU");

        if let AdditionalAddress::MbapHeader(mbap_header) =
            tcp_message.modbus_message.additional_address
        {
            assert_eq!(mbap_header.transaction_id, 0x0001);
            assert_eq!(mbap_header.protocol_id, 0x0000);
            assert_eq!(mbap_header.length, 0x00FE);
            assert_eq!(mbap_header.unit_id, 0xFF);
        } else {
            panic!("Expected MbapHeader");
        }

        assert_eq!(
            tcp_message.modbus_message.function_code(),
            FunctionCode::WriteMultipleRegisters
        );
        assert_eq!(tcp_message.modbus_message.data_len(), 252);
        assert_eq!(tcp_message.modbus_message.data().as_slice(), pdu_data_expected_slice);
    }

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with a PDU containing only a function code (no data).
    ///
    /// This tests deserialization for requests like Report Server ID.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 6.13 (Report Server ID).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_no_pdu_data() {
        #[rustfmt::skip]
        let adu_bytes: [u8; 8] = [
            0x00, 0x02, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x02, // Length (1 byte Unit ID + 1 byte FC)
            0x0A,       // Unit ID
            0x11,       // Function Code (Report Server ID)
        ];

        let tcp_message = ModbusTcpMessage::from_adu_bytes(&adu_bytes)
            .expect("Failed to deserialize ADU with no data");

        if let AdditionalAddress::MbapHeader(mbap_header) =
            tcp_message.modbus_message.additional_address
        {
            assert_eq!(mbap_header.transaction_id, 0x0002);
            assert_eq!(mbap_header.protocol_id, 0x0000);
            assert_eq!(mbap_header.length, 0x0002);
            assert_eq!(mbap_header.unit_id, 0x0A);
        } else {
            panic!("Expected MbapHeader");
        }

        assert_eq!(
            tcp_message.modbus_message.function_code(),
            FunctionCode::ReportServerId
        );
        assert_eq!(tcp_message.modbus_message.data_len(), 0);
    }

    /// Test case: `ModbusTcpMessage` round-trip serialization and deserialization.
    ///
    /// This test ensures that a message can be serialized to bytes and then deserialized
    /// back into an equivalent message, verifying data integrity.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_round_trip() {
        let mbap_header = MbapHeader {
            transaction_id: 0xABCD,
            protocol_id: 0x0000,
            length: 0x0006, // Placeholder
            unit_id: 0xEE,
        };

        let mut pdu_data_vec: Vec<u8, MAX_PDU_DATA_LEN> = Vec::new();
        pdu_data_vec.push(0x01).unwrap();
        pdu_data_vec.push(0x02).unwrap();
        pdu_data_vec.push(0x03).unwrap();
        pdu_data_vec.push(0x04).unwrap();
        pdu_data_vec.push(0x05).unwrap();

        let original_pdu = Pdu::new(
            FunctionCode::ReadWriteMultipleRegisters,
            pdu_data_vec,
            5,
        );

        let original_message = ModbusTcpMessage::new(mbap_header, original_pdu);

        // Serialize
        let adu_bytes = original_message
            .to_adu_bytes()
            .expect("Failed to serialize for round-trip");

        // Deserialize
        let deserialized_message = ModbusTcpMessage::from_adu_bytes(&adu_bytes)
            .expect("Failed to deserialize for round-trip");

        // Verify MBAP header
        if let (
            AdditionalAddress::MbapHeader(orig_mbap),
            AdditionalAddress::MbapHeader(deser_mbap),
        ) = (
            original_message.modbus_message.additional_address,
            deserialized_message.modbus_message.additional_address,
        ) {
            assert_eq!(orig_mbap.transaction_id, deser_mbap.transaction_id);
            assert_eq!(orig_mbap.protocol_id, deser_mbap.protocol_id);
            // The length field is recalculated during serialization, so compare the actual length of the PDU + Unit ID.
            assert_eq!(original_message.modbus_message.pdu.to_bytes().unwrap().len() as u16 + 1, deser_mbap.length);
            assert_eq!(orig_mbap.unit_id, deser_mbap.unit_id);
        } else {
            panic!("Expected MbapHeader for both original and deserialized messages");
        }

        // Verify PDU
        assert_eq!(
            original_message.modbus_message.function_code(),
            deserialized_message.modbus_message.function_code()
        );
        assert_eq!(
            original_message.modbus_message.data_len(),
            deserialized_message.modbus_message.data_len()
        );

        assert_eq!(
            original_message.modbus_message.data().as_slice(),
            deserialized_message.modbus_message.data().as_slice()
        );
    }

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with an invalid ADU (too short).
    ///
    /// This tests the error handling when the provided byte slice is too short to be a valid Modbus TCP ADU.
    /// Minimum ADU length is 8 bytes (7 MBAP + 1 FC).
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_invalid_length_too_short() {
        let adu_bytes: [u8; 7] = [0x12, 0x34, 0x00, 0x00, 0x00, 0x01, 0x01]; // Incomplete MBAP header + FC
        let err = ModbusTcpMessage::from_adu_bytes(&adu_bytes)
            .expect_err("Should return error for invalid length");
        assert_eq!(err, MbusError::InvalidPduLength);
    }

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with an invalid ADU (invalid protocol ID).
    ///
    /// This tests the error handling when the Protocol ID in the MBAP header is not 0x0000.
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_invalid_protocol_id() {
        let adu_bytes: [u8; 8] = [0x12, 0x34, 0x00, 0x01, 0x00, 0x01, 0x01, 0x03]; // Invalid Protocol ID (0x0001)
        let err = ModbusTcpMessage::from_adu_bytes(&adu_bytes)
            .expect_err("Should return error for invalid protocol ID");
        assert_eq!(err, MbusError::ParseError);
    }

    /// Test case: `ModbusTcpMessage::from_adu_bytes` with an invalid ADU (length field mismatch).
    ///
    /// This tests the error handling when the Length field in the MBAP header does not match the actual length of the remaining ADU.
    /// Here, the length field (0x0003) indicates 3 bytes follow (Unit ID + PDU), but the actual slice has 4 bytes (Unit ID + FC + 1 Data).
    ///
    /// Modbus Specification Reference: V1.1b3, Section 4.2 (Modbus TCP ADU).
    #[test]
    fn test_modbus_tcp_message_from_adu_bytes_invalid_length_field() {
        #[rustfmt::skip]
        let adu_bytes: [u8; 9] = [
            0x12, 0x34, // Transaction ID
            0x00, 0x00, // Protocol ID
            0x00, 0x02, // Length (Incorrect, should be 3 for Unit ID + FC + 1 data byte)
            0x01,       // Unit ID
            0x03,       // Function Code
            0x00,       // Data
        ];
        let err = ModbusTcpMessage::from_adu_bytes(&adu_bytes)
            .expect_err("Should return error for invalid length field");
        assert_eq!(err, MbusError::InvalidPduLength);
    }
}
