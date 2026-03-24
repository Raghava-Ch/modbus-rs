//! # Modbus Diagnostic Services
//!
//! This module provides the implementation for Modbus diagnostic and maintenance services.
//! It supports standard function codes used for device identification, health checks,
//! and transport-specific diagnostics.
//!
//! ## Supported Function Codes
//! - **FC 08 (0x08): Diagnostics** - Provides a series of tests for checking the communication
//!   system between a client and a server, or for checking various internal error conditions
//!   within a server.
//! - **FC 11 (0x0B): Get Comm Event Counter** - Used to get a status word and an event count
//!   from the remote device's communication event counter.
//! - **FC 12 (0x0C): Get Comm Event Log** - Used to get a status word, event count, message
//!   count, and a field of event bytes from the remote device.
//! - **FC 07 (0x07): Read Exception Status** - Used to read the contents of eight Exception
//!   Status outputs in a remote device.
//! - **FC 17 (0x11): Report Server ID** - Used to read the description of the type, the
//!   current status, and other information specific to a remote device.
//! - **FC 43 / MEI 14 (0x2B / 0x0E): Read Device Identification** - Allows reading
//!   identification and additional information relative to the physical and functional
//!   description of a remote device.
//!
//! ## Architecture
//! - `apis`: High-level methods exposed to the `ClientServices` struct.
//! - `request`: Logic for compiling request Protocol Data Units (PDUs).
//! - `response`: Logic for parsing and validating response PDUs.
//! - `service`: Internal orchestration of diagnostic operations.
//!
//! This module is designed to be `no_std` compatible, utilizing `heapless` collections
//! for memory efficiency.

mod request;
mod response;

pub use mbus_core::models::diagnostic::*;
mod apis;
mod service;

#[cfg(test)]
mod tests {
    use heapless::Vec;

    use crate::services::diagnostic::{request::ReqPduCompiler, response::ResponseParser};
    use mbus_core::{
        data_unit::common::Pdu,
        errors::MbusError,
        function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType, FunctionCode},
        models::diagnostic::{ConformityLevel, DeviceIdObject, ObjectId, ReadDeviceIdCode},
    };

    #[test]
    fn test_read_exception_status_request() {
        let pdu = ReqPduCompiler::read_exception_status_request().unwrap();
        assert_eq!(pdu.function_code(), FunctionCode::ReadExceptionStatus);
        assert_eq!(pdu.data_len(), 0);
    }

    #[test]
    fn test_diagnostics_request() {
        let sub_function = DiagnosticSubFunction::ReturnQueryData; // Return Query Data
        let data = [0xA5A5, 0x5A5A];
        let pdu = ReqPduCompiler::diagnostics_request(sub_function, &data).unwrap();

        assert_eq!(pdu.function_code(), FunctionCode::Diagnostics);
        assert_eq!(pdu.data_len(), 6); // 2 sub-func + 4 data
        assert_eq!(pdu.data().as_slice(), &[0x00, 0x00, 0xA5, 0xA5, 0x5A, 0x5A]);
    }

    #[test]
    fn test_parse_diagnostics_response() {
        let data = [0x00, 0x00, 0xA5, 0xA5, 0x5A, 0x5A];
        let pdu = Pdu::new(
            FunctionCode::Diagnostics,
            Vec::from_slice(&data).unwrap(),
            6,
        );
        let (sub_func, values) = ResponseParser::parse_diagnostics_response(&pdu).unwrap();
        assert_eq!(sub_func, 0x0000);
        assert_eq!(values.as_slice(), &[0xA5A5, 0x5A5A]);
    }

    #[test]
    fn test_read_device_identification_request() {
        let pdu = ReqPduCompiler::read_device_identification_request(
            ReadDeviceIdCode::Basic,
            ObjectId::from(0x00),
        )
        .unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::EncapsulatedInterfaceTransport
        );
        assert_eq!(pdu.data().as_slice(), &[0x0E, 0x01, 0x00]);
    }

    #[test]
    fn test_parse_read_device_identification_response() {
        // MEI(0E), Code(01), Conf(81), More(00), Next(00), Num(02)
        // Obj1: Id(00), Len(03), Val("Foo")
        // Obj2: Id(01), Len(03), Val("Bar")
        let data = [
            0x0E, 0x01, 0x81, 0x00, 0x00, 0x02, 0x00, 0x03, 0x46, 0x6F, 0x6F, 0x01, 0x03, 0x42,
            0x61, 0x72,
        ];
        let pdu = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data).unwrap(),
            data.len() as u8,
        );
        let resp = ResponseParser::parse_read_device_identification_response(
            ReadDeviceIdCode::Basic,
            &pdu,
        )
        .unwrap();

        assert_eq!(resp.read_device_id_code, ReadDeviceIdCode::Basic);
        assert_eq!(
            resp.conformity_level,
            ConformityLevel::BasicStreamAndIndividual
        );
        assert_eq!(resp.more_follows, false);
        assert_eq!(resp.next_object_id, ObjectId::from(0x00));

        let objects: Vec<DeviceIdObject, 10> = resp.objects().map(|r| r.unwrap()).collect();
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[0].object_id, ObjectId::from(0x00));
        assert_eq!(objects[0].value.as_slice(), b"Foo");
        assert_eq!(objects[1].object_id, ObjectId::from(0x01));
        assert_eq!(objects[1].value.as_slice(), b"Bar");
    }

    #[test]
    fn test_parse_read_device_identification_response_malformed() {
        // Case 1: Truncated header (missing Number of Objects)
        let data_short = [0x0E, 0x01, 0x81, 0x00, 0x00];
        let pdu_short = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data_short).unwrap(),
            5,
        );
        assert_eq!(
            ResponseParser::parse_read_device_identification_response(
                ReadDeviceIdCode::Basic,
                &pdu_short
            )
            .unwrap_err(),
            MbusError::InvalidDataLen
        );

        // Case 2: Object length exceeds available data
        // Num Objects = 1. Obj Id = 00. Obj Len = 05. But only 3 bytes ("Foo") follow.
        let data_overflow = [
            0x0E, 0x01, 0x81, 0x00, 0x00, 0x01, 0x00, 0x05, 0x46, 0x6F, 0x6F,
        ];
        let pdu_overflow = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data_overflow).unwrap(),
            data_overflow.len() as u8,
        );
        assert_eq!(
            ResponseParser::parse_read_device_identification_response(
                ReadDeviceIdCode::Basic,
                &pdu_overflow
            )
            .unwrap_err(),
            MbusError::InvalidPduLength
        );
    }

    #[test]
    fn test_encapsulated_interface_transport_request() {
        let mei_type = EncapsulatedInterfaceType::CanopenGeneralReference;
        let data = [0x01, 0x02, 0x03];
        let pdu =
            ReqPduCompiler::encapsulated_interface_transport_request(mei_type, &data).unwrap();
        assert_eq!(
            pdu.function_code(),
            FunctionCode::EncapsulatedInterfaceTransport
        );
        assert_eq!(pdu.data().as_slice(), &[0x0D, 0x01, 0x02, 0x03]);
    }

    #[test]
    fn test_parse_encapsulated_interface_transport_response() {
        let data = [0x0D, 0x01, 0x02, 0x03];
        let pdu = Pdu::new(
            FunctionCode::EncapsulatedInterfaceTransport,
            Vec::from_slice(&data).unwrap(),
            4,
        );
        let (mei_type, resp_data) =
            ResponseParser::parse_encapsulated_interface_transport_response(&pdu).unwrap();
        assert_eq!(mei_type, EncapsulatedInterfaceType::CanopenGeneralReference);
        assert_eq!(resp_data.as_slice(), &[0x01, 0x02, 0x03]);
    }
}
