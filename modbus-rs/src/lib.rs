pub use heapless;

pub use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN};
pub use mbus_core::errors::MbusError;
pub use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
pub use mbus_core::transport::checksum::crc16;
pub use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    ModbusTcpConfig, Parity, SerialMode, TimeKeeper, Transport, TransportError, TransportType,
    UnitIdOrSlaveAddr,
};

#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
pub use mbus_serial::StdSerialTransport;
#[cfg(feature = "tcp")]
pub use mbus_tcp::StdTcpTransport;

#[cfg(feature = "client")]
pub use mbus_client::app::*;
#[cfg(feature = "client")]
pub use mbus_client::services::{ClientServices, SerialClientServices};

#[cfg(all(feature = "client", feature = "coils"))]
pub use mbus_client::services::coil::{Coils, MAX_COIL_BYTES, MAX_COILS_PER_PDU};
#[cfg(all(feature = "client", feature = "diagnostics"))]
pub use mbus_client::services::diagnostic::{
    BasicObjectId, ConformityLevel, DeviceIdObject, DeviceIdObjectIterator,
    DeviceIdentificationResponse, ExtendedObjectId, ObjectId, ReadDeviceIdCode, RegularObjectId,
};
#[cfg(all(feature = "client", feature = "discrete-inputs"))]
pub use mbus_client::services::discrete_input::{
    DiscreteInputs, MAX_DISCRETE_INPUT_BYTES, MAX_DISCRETE_INPUTS_PER_PDU,
};
#[cfg(all(feature = "client", feature = "fifo"))]
pub use mbus_client::services::fifo_queue::{FifoQueue, MAX_FIFO_QUEUE_COUNT_PER_PDU};
#[cfg(all(feature = "client", feature = "file-record"))]
pub use mbus_client::services::file_record::{
    FILE_RECORD_REF_TYPE, MAX_SUB_REQUESTS_PER_PDU, SUB_REQ_PARAM_BYTE_LEN, SubRequest,
    SubRequestParams,
};
#[cfg(all(feature = "client", feature = "registers"))]
pub use mbus_client::services::register::{MAX_REGISTERS_PER_PDU, Registers};
