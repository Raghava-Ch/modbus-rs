//! Integration tests for FC03 exception response handling.
//!
//! These tests verify that the server correctly generates and sends
//! Modbus exception ADUs when errors occur during FC03 processing.

#[cfg(all(test, feature = "holding-registers"))]
mod fc03_exception_tests {
    use mbus_core::errors::{ExceptionCode, MbusError};
    use mbus_core::function_codes::public::FunctionCode;
    use mbus_core::transport::{TransportType, UnitIdOrSlaveAddr};
    use mbus_server::services::exception;
    // ────────────────────────── Tests for Exception Code Mapping ────────────────────────────

    #[test]
    fn exception_code_mapping_invalid_address() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidAddress);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_invalid_offset() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidOffset);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_invalid_data_len() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidDataLen);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_invalid_quantity() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidQuantity);
        assert_eq!(code, ExceptionCode::IllegalDataValue);
    }

    #[test]
    fn exception_code_mapping_invalid_value() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidValue);
        assert_eq!(code, ExceptionCode::IllegalDataValue);
    }

    #[test]
    fn exception_code_mapping_invalid_byte_count() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidByteCount);
        assert_eq!(code, ExceptionCode::IllegalDataValue);
    }

    #[test]
    fn exception_code_mapping_parse_error() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::ParseError);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_basic_parse_error() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::BasicParseError);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_invalid_pdu_length() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidPduLength);
        assert_eq!(code, ExceptionCode::IllegalDataAddress);
    }

    #[test]
    fn exception_code_mapping_invalid_function_code() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::InvalidFunctionCode);
        assert_eq!(code, ExceptionCode::IllegalFunction);
    }

    #[test]
    fn exception_code_mapping_unsupported_function() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::UnsupportedFunction(42));
        assert_eq!(code, ExceptionCode::IllegalFunction);
    }

    #[test]
    fn exception_code_mapping_io_error() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::IoError);
        assert_eq!(code, ExceptionCode::ServerDeviceFailure);
    }

    #[test]
    fn exception_code_mapping_connection_lost() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::ConnectionLost);
        assert_eq!(code, ExceptionCode::ServerDeviceFailure);
    }

    #[test]
    fn exception_code_mapping_timeout() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::Timeout);
        assert_eq!(code, ExceptionCode::ServerDeviceFailure);
    }

    #[test]
    fn exception_code_mapping_buffer_too_small() {
        let fc = FunctionCode::ReadHoldingRegisters;
        let code = fc.exception_code_for_error(&MbusError::BufferTooSmall);
        assert_eq!(code, ExceptionCode::ServerDeviceFailure);
    }

    // ────────────────────────── Tests for Exception ADU Building ────────────────────────────

    #[test]
    fn build_exception_adu_creates_valid_frame() {
        let adu = exception::build_exception_adu(
            1, // txn_id
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
            TransportType::StdTcp,
        );

        assert!(adu.is_ok(), "ADU building should succeed");
        let adu_bytes = adu.unwrap();
        assert!(!adu_bytes.is_empty(), "ADU should not be empty");
    }

    #[test]
    fn exception_adu_has_error_bit_set_tcp() {
        let adu = exception::build_exception_adu(
            1,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
            TransportType::StdTcp,
        );

        let adu_bytes = adu.unwrap();
        // For TCP: MBAP is 7 bytes, function code at offset 7
        assert!(
            adu_bytes.len() >= 9,
            "ADU length should be at least 9 bytes"
        );
        assert_eq!(adu_bytes[7] & 0x80, 0x80, "Error bit (0x80) must be set");
        assert_eq!(
            adu_bytes[7] & 0x7F,
            0x03,
            "Base function code should be 0x03"
        );
    }

    #[test]
    fn exception_adu_exception_code_correct_tcp() {
        let adu = exception::build_exception_adu(
            1,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
            TransportType::StdTcp,
        );

        let adu_bytes = adu.unwrap();
        // For TCP: MBAP (7 bytes) + function_code_with_error (1 byte) + exception_code (1 byte)
        assert_eq!(
            adu_bytes[8], 0x02,
            "Exception code should be 0x02 (Illegal Data Address)"
        );
    }

    #[test]
    fn exception_adu_server_device_failure_code() {
        let adu = exception::build_exception_adu(
            1,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::ServerDeviceFailure,
            TransportType::StdTcp,
        );

        let adu_bytes = adu.unwrap();
        assert_eq!(
            adu_bytes[8], 0x04,
            "Exception code should be 0x04 (Server Device Failure)"
        );
    }

    #[test]
    fn exception_adu_illegal_data_value_code() {
        let adu = exception::build_exception_adu(
            1,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataValue,
            TransportType::StdTcp,
        );

        let adu_bytes = adu.unwrap();
        assert_eq!(
            adu_bytes[8], 0x03,
            "Exception code should be 0x03 (Illegal Data Value)"
        );
    }

    #[test]
    fn exception_adu_illegal_function_code() {
        let adu = exception::build_exception_adu(
            1,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalFunction,
            TransportType::StdTcp,
        );

        let adu_bytes = adu.unwrap();
        assert_eq!(
            adu_bytes[8], 0x01,
            "Exception code should be 0x01 (Illegal Function)"
        );
    }

    #[test]
    fn exception_adu_transaction_id_preserved() {
        // Different transaction IDs should appear in the TCPMBAP
        let adu1 = exception::build_exception_adu(
            100,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
            TransportType::StdTcp,
        );

        let adu2 = exception::build_exception_adu(
            200,
            UnitIdOrSlaveAddr::new(1).unwrap(),
            FunctionCode::ReadHoldingRegisters,
            ExceptionCode::IllegalDataAddress,
            TransportType::StdTcp,
        );

        let bytes1 = adu1.unwrap();
        let bytes2 = adu2.unwrap();

        // Transaction ID is in bytes 0-1 (big-endian)
        // They should be different
        assert_ne!(
            bytes1[0..2],
            bytes2[0..2],
            "Different transaction IDs should produce different frames"
        );
    }
}
