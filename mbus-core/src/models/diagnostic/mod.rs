//! # Modbus Diagnostic Models
//!
//! This module contains data structures and logic for Modbus diagnostic services,
//! primarily focusing on **Function Code 43 (0x2B)**: Encapsulated Interface Transport.
//!
//! Specifically, it implements the **Read Device Identification (MEI Type 0x0E)**
//! protocol, which allows a client to retrieve vendor name, product code, and
//! revision information from a remote device.
//!
//! The module provides strongly-typed representations of Object IDs, Conformity Levels,
//! and a memory-efficient iterator for parsing identification objects in `no_std` environments.
//! # Example
//! ```
//! # use mbus_core::models::diagnostic::{DeviceIdentificationResponse, ReadDeviceIdCode, ConformityLevel, ObjectId};
//! # let resp = DeviceIdentificationResponse {
//! #     read_device_id_code: ReadDeviceIdCode::Basic,
//! #     conformity_level: ConformityLevel::BasicStreamAndIndividual,
//! #     more_follows: false,
//! #     next_object_id: ObjectId::from(0x00),
//! #     objects_data: [0; 252],
//! #     number_of_objects: 0,
//! # };
//! // Assuming a response has been received and parsed into `resp`
//! for obj_result in resp.objects() {
//!     let obj = obj_result.expect("Valid object");
//!     println!("Object ID: {}, Value: {:?}", obj.object_id, obj.value);
//! }
//! ```

mod model;
pub use model::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::errors::MbusError;

    #[test]
    fn test_extended_object_id() {
        assert!(ExtendedObjectId::new(0x7F).is_none()); // Out of bounds
        assert!(ExtendedObjectId::new(0x80).is_some()); // Starts at 0x80
        assert!(ExtendedObjectId::new(0xFF).is_some()); // Ends at 0xFF
        assert_eq!(ExtendedObjectId::new(0x80).unwrap().value(), 0x80);
    }

    #[test]
    fn test_read_device_id_code() {
        assert_eq!(
            ReadDeviceIdCode::try_from(0x01).unwrap(),
            ReadDeviceIdCode::Basic
        );
        assert_eq!(
            ReadDeviceIdCode::try_from(0x02).unwrap(),
            ReadDeviceIdCode::Regular
        );
        assert_eq!(
            ReadDeviceIdCode::try_from(0x03).unwrap(),
            ReadDeviceIdCode::Extended
        );
        assert_eq!(
            ReadDeviceIdCode::try_from(0x04).unwrap(),
            ReadDeviceIdCode::Specific
        );
        assert_eq!(
            ReadDeviceIdCode::try_from(0x05).unwrap_err(),
            MbusError::InvalidDeviceIdCode
        );
    }

    #[test]
    fn test_conformity_level() {
        assert_eq!(
            ConformityLevel::try_from(0x01).unwrap(),
            ConformityLevel::BasicStreamOnly
        );
        assert_eq!(
            ConformityLevel::try_from(0x81).unwrap(),
            ConformityLevel::BasicStreamAndIndividual
        );
        assert_eq!(
            ConformityLevel::try_from(0x04).unwrap_err(),
            MbusError::ParseError
        );
    }

    #[test]
    fn test_device_id_object_iterator_valid_parse() {
        let mut objects_data = [0u8; crate::data_unit::common::MAX_PDU_DATA_LEN];

        // Prepare mock object stream: [Id, Length, Value...]
        // Object 1: VendorName (0x00), Length 3, "Foo"
        objects_data[0] = 0x00;
        objects_data[1] = 0x03;
        objects_data[2..5].copy_from_slice(b"Foo");

        // Object 2: ProductCode (0x01), Length 3, "Bar"
        objects_data[5] = 0x01;
        objects_data[6] = 0x03;
        objects_data[7..10].copy_from_slice(b"Bar");

        let response = DeviceIdentificationResponse {
            read_device_id_code: ReadDeviceIdCode::Basic,
            conformity_level: ConformityLevel::BasicStreamAndIndividual,
            more_follows: false,
            next_object_id: ObjectId::from(0x00),
            objects_data,
            number_of_objects: 2,
        };

        let mut iterator = response.objects();

        // Validate first object correctly parses and yields
        let obj1 = iterator.next().expect("Should yield first object").unwrap();
        assert_eq!(obj1.object_id, ObjectId::Basic(BasicObjectId::VendorName));
        assert_eq!(obj1.value.as_slice(), b"Foo");

        // Validate second object correctly parses and yields
        let obj2 = iterator
            .next()
            .expect("Should yield second object")
            .unwrap();
        assert_eq!(obj2.object_id, ObjectId::Basic(BasicObjectId::ProductCode));
        assert_eq!(obj2.value.as_slice(), b"Bar");

        // Ensure iterator correctly concludes
        assert!(iterator.next().is_none());
    }

    #[test]
    fn test_device_id_object_iterator_invalid_length() {
        // Setup a truncated payload to simulate network error or bad Modbus server response
        let mut objects_data = [0u8; crate::data_unit::common::MAX_PDU_DATA_LEN];
        objects_data[0] = 0x00;
        objects_data[1] = 0xFF; // Declares 255 bytes of data, which exceeds the buffer maximum size
        objects_data[2] = b'F';

        let response = DeviceIdentificationResponse {
            read_device_id_code: ReadDeviceIdCode::Basic,
            conformity_level: ConformityLevel::BasicStreamAndIndividual,
            more_follows: false,
            next_object_id: ObjectId::from(0x00),
            objects_data,
            number_of_objects: 1,
        };

        let mut iterator = response.objects();

        // It should catch the out-of-bounds error and return InvalidPduLength
        let res = iterator
            .next()
            .expect("Should return error instead of None");
        assert_eq!(res.unwrap_err(), MbusError::InvalidPduLength);
    }
}
