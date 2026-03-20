use crate::{data_unit::common::MAX_PDU_DATA_LEN, errors::MbusError};
use core::fmt;
use heapless::Vec;

/// Represents an object ID.
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdObject {
    /// The ID of the object.
    pub object_id: ObjectId,
    /// The value of the object.
    pub value: Vec<u8, MAX_PDU_DATA_LEN>,
}

/// An iterator over the device identification objects.
pub struct DeviceIdObjectIterator<'a> {
    data: &'a [u8],
    offset: usize,
    /// The number of objects already parsed.
    count: u8,
    /// The total number of objects expected.
    total: u8,
}

impl<'a> Iterator for DeviceIdObjectIterator<'a> {
    type Item = Result<DeviceIdObject, MbusError>;

    /// Advances the iterator and returns the next device ID object.
    fn next(&mut self) -> Option<Self::Item> {
        if self.count >= self.total {
            return None;
        }

        // Parsing logic is handled internally in the iterator step
        // We reuse the parsing logic from the original implementation but applied incrementally
        self.parse_next()
    }
}

impl<'a> DeviceIdObjectIterator<'a> {
    /// Parses the next `DeviceIdObject` from the raw objects data buffer.
    ///
    /// Each object in the stream consists of:
    /// - Object Id (1 byte)
    /// - Object Length (1 byte)
    /// - Object Value (N bytes)
    fn parse_next(&mut self) -> Option<Result<DeviceIdObject, MbusError>> {
        // Check if there is enough data for the 2-byte header (Id + Length)
        if self.offset + 2 > self.data.len() {
            return Some(Err(MbusError::InvalidPduLength));
        }
        let obj_id = ObjectId::from(self.data[self.offset]);
        let obj_len = self.data[self.offset + 1] as usize;
        self.offset += 2; // Move past the header

        // Ensure the remaining data contains the full object value
        if self.offset + obj_len > self.data.len() {
            return Some(Err(MbusError::InvalidPduLength));
        }

        let mut value = Vec::new();
        // Copy the object value into the heapless::Vec
        if value
            .extend_from_slice(&self.data[self.offset..self.offset + obj_len])
            .is_err()
        {
            return Some(Err(MbusError::BufferTooSmall));
        }

        self.offset += obj_len;
        self.count += 1;

        Some(Ok(DeviceIdObject {
            object_id: obj_id,
            value,
        }))
    }
}

/// Represents a response to a Read Device Identification request (FC 43 / MEI 0E).
#[derive(Debug, Clone, PartialEq)]
pub struct DeviceIdentificationResponse {
    /// The code defining the type of access.
    pub read_device_id_code: ReadDeviceIdCode,
    /// The conformity level of the response.
    pub conformity_level: ConformityLevel,
    /// Indicates if there are more objects to follow.
    pub more_follows: bool,
    /// The ID of the next object in the response.
    pub next_object_id: ObjectId,
    /// The raw data of the objects.
    pub objects_data: Vec<u8, MAX_PDU_DATA_LEN>,
    /// The number of objects returned in the response.
    pub number_of_objects: u8,
}

impl DeviceIdentificationResponse {
    /// Returns an iterator over the device identification objects.
    pub fn objects(&self) -> DeviceIdObjectIterator<'_> {
        DeviceIdObjectIterator {
            data: &self.objects_data,
            offset: 0,
            count: 0,
            total: self.number_of_objects,
        }
    }
}

/// Object IDs for Basic Device Identification.
///
/// These objects are mandatory for the Basic conformity level.
/// Access type: Stream (ReadDeviceIdCode 0x01, 0x02, 0x03) or Individual (ReadDeviceIdCode 0x04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum BasicObjectId {
    /// Vendor Name (Mandatory). Object ID 0x00.
    VendorName = 0x00,
    /// Product Code (Mandatory). Object ID 0x01.
    ProductCode = 0x01,
    /// Major Minor Revision (Mandatory). Object ID 0x02.
    MajorMinorRevision = 0x02,
}

impl TryFrom<u8> for BasicObjectId {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x00 => Ok(BasicObjectId::VendorName),
            0x01 => Ok(BasicObjectId::ProductCode),
            0x02 => Ok(BasicObjectId::MajorMinorRevision),
            _ => Err(MbusError::InvalidAddress),
        }
    }
}

impl fmt::Display for BasicObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BasicObjectId::VendorName => write!(f, "VendorName"),
            BasicObjectId::ProductCode => write!(f, "ProductCode"),
            BasicObjectId::MajorMinorRevision => write!(f, "MajorMinorRevision"),
        }
    }
}

/// Object IDs for Regular Device Identification.
///
/// These objects are optional but defined in the standard.
/// Access type: Stream (ReadDeviceIdCode 0x02, 0x03) or Individual (ReadDeviceIdCode 0x04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum RegularObjectId {
    /// Vendor URL (Optional). Object ID 0x03.
    VendorUrl = 0x03,
    /// Product Name (Optional). Object ID 0x04.
    ProductName = 0x04,
    /// Model Name (Optional). Object ID 0x05.
    ModelName = 0x05,
    /// User Application Name (Optional). Object ID 0x06.
    UserApplicationName = 0x06,
}

impl TryFrom<u8> for RegularObjectId {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x03 => Ok(RegularObjectId::VendorUrl),
            0x04 => Ok(RegularObjectId::ProductName),
            0x05 => Ok(RegularObjectId::ModelName),
            0x06 => Ok(RegularObjectId::UserApplicationName),
            _ => Err(MbusError::InvalidAddress),
        }
    }
}

impl fmt::Display for RegularObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RegularObjectId::VendorUrl => write!(f, "VendorUrl"),
            RegularObjectId::ProductName => write!(f, "ProductName"),
            RegularObjectId::ModelName => write!(f, "ModelName"),
            RegularObjectId::UserApplicationName => write!(f, "UserApplicationName"),
        }
    }
}

/// Extended Object IDs.
///
/// Range 0x80 - 0xFF. These are private/vendor-specific.
/// Access type: Stream (ReadDeviceIdCode 0x03) or Individual (ReadDeviceIdCode 0x04).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtendedObjectId(u8);

impl ExtendedObjectId {
    /// Creates a new `ExtendedObjectId`.
    ///
    /// Returns `None` if the id is not in the range 0x80..=0xFF.
    pub fn new(id: u8) -> Option<Self> {
        if (0x80..=0xFF).contains(&id) {
            Some(Self(id))
        } else {
            None
        }
    }

    /// Returns the raw object ID.
    pub fn value(&self) -> u8 {
        self.0
    }
}

/// Read Device ID Code (Function Code 43 / 14).
///
/// Defines the type of access requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum ReadDeviceIdCode {
    /// An invalid or uninitialized value. In practical never should have reached here.
    #[default]
    Err,
    /// Request to get the basic device identification (stream access).
    Basic = 0x01,
    /// Request to get the regular device identification (stream access).
    Regular = 0x02,
    /// Request to get the extended device identification (stream access).
    Extended = 0x03,
    /// Request to get one specific identification object (individual access).
    Specific = 0x04,
}

impl TryFrom<u8> for ReadDeviceIdCode {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(ReadDeviceIdCode::Basic),
            0x02 => Ok(ReadDeviceIdCode::Regular),
            0x03 => Ok(ReadDeviceIdCode::Extended),
            0x04 => Ok(ReadDeviceIdCode::Specific),
            _ => Err(MbusError::InvalidDeviceIdCode),
        }
    }
}

/// Conformity Level returned in the response.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ConformityLevel {
    /// Basic identification (stream access only).
    BasicStreamOnly = 0x01,
    /// Regular identification (stream access only).
    RegularStreamOnly = 0x02,
    /// Extended identification (stream access only).
    ExtendedStreamOnly = 0x03,
    /// Basic identification (stream access and individual access).
    BasicStreamAndIndividual = 0x81,
    /// Regular identification (stream access and individual access).
    RegularStreamAndIndividual = 0x82,
    /// Extended identification (stream access and individual access).
    ExtendedStreamAndIndividual = 0x83,
}

impl TryFrom<u8> for ConformityLevel {
    type Error = MbusError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x01 => Ok(ConformityLevel::BasicStreamOnly),
            0x02 => Ok(ConformityLevel::RegularStreamOnly),
            0x03 => Ok(ConformityLevel::ExtendedStreamOnly),
            0x81 => Ok(ConformityLevel::BasicStreamAndIndividual),
            0x82 => Ok(ConformityLevel::RegularStreamAndIndividual),
            0x83 => Ok(ConformityLevel::ExtendedStreamAndIndividual),
            _ => Err(MbusError::ParseError),
        }
    }
}

/// Represents any valid Modbus Device Identification Object ID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ObjectId {
    /// Unintentional use, In practical never should have reached here.
    #[default]
    Err,
    /// Basic Device Identification Object IDs (0x00 - 0x02).
    Basic(BasicObjectId),
    /// Regular Device Identification Object IDs (0x03 - 0x06).
    Regular(RegularObjectId),
    /// Extended Device Identification Object IDs (0x80 - 0xFF).
    Extended(ExtendedObjectId),
    /// Reserved range (0x07 - 0x7F).
    Reserved(u8),
}

impl From<u8> for ObjectId {
    fn from(id: u8) -> Self {
        if let Ok(basic) = BasicObjectId::try_from(id) {
            ObjectId::Basic(basic)
        } else if let Ok(regular) = RegularObjectId::try_from(id) {
            ObjectId::Regular(regular)
        } else if let Some(extended) = ExtendedObjectId::new(id) {
            ObjectId::Extended(extended)
        } else {
            ObjectId::Reserved(id)
        }
    }
}

impl fmt::Display for ObjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ObjectId::Basic(id) => write!(f, "Basic({})", id),
            ObjectId::Regular(id) => write!(f, "Regular({})", id),
            ObjectId::Extended(id) => write!(f, "Extended({:#04X})", id.value()),
            ObjectId::Reserved(id) => write!(f, "Reserved({:#04X})", id),
            ObjectId::Err => write!(f, "Err, In practical never should have reached here."),
        }
    }
}

impl From<ObjectId> for u8 {
    fn from(oid: ObjectId) -> u8 {
        match oid {
            ObjectId::Basic(id) => id as u8,
            ObjectId::Regular(id) => id as u8,
            ObjectId::Extended(id) => id.value(),
            ObjectId::Reserved(id) => id,
            ObjectId::Err => 0, // In practical never should have reached here.
        }
    }
}
