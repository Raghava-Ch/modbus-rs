// When none of the std-requiring features are enabled this crate is no_std compatible.
// The doc build is excluded so rustdoc can use std freely for link resolution.
// `std-required` is an internal umbrella feature implied by every feature that needs std
// (network-tcp, serial-rtu, serial-ascii, async, logging, server). Adding a new
// std-requiring feature only requires adding `"std-required"` to its entry in Cargo.toml.
#![cfg_attr(not(any(doc, feature = "std-required")), no_std)]

pub use heapless;

pub use mbus_core;
pub use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, MAX_PDU_DATA_LEN};
pub use mbus_core::errors::MbusError;
pub use mbus_core::function_codes::public::{DiagnosticSubFunction, EncapsulatedInterfaceType};
pub use mbus_core::transport::checksum::crc16;
pub use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    ModbusTcpConfig, Parity, SerialMode, TimeKeeper, Transport, TransportError, TransportType,
    UnitIdOrSlaveAddr,
};

#[cfg(all(feature = "network-tcp", feature = "async"))]
pub use mbus_network::TokioTcpTransport;
#[cfg(feature = "network-tcp")]
pub use mbus_network::{StdTcpServerTransport, StdTcpTransport};
#[cfg(feature = "serial-rtu")]
pub use mbus_serial::StdRtuTransport;
#[cfg(feature = "serial-ascii")]
pub use mbus_serial::StdAsciiTransport;
#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
pub use mbus_serial::StdSerialTransport;

#[cfg(feature = "server")]
pub use mbus_server::async_modbus_app;
#[cfg(feature = "server")]
pub use mbus_server::modbus_app;
#[cfg(feature = "server")]
pub use mbus_server::{
    ClockFn, ForwardingApp, ModbusAppAccess, ModbusAppHandler, OverflowPolicy, RequestPriority,
    ResilienceConfig, ServerExceptionHandler, ServerServices, TimeoutConfig,
};

#[cfg(all(feature = "server", feature = "diagnostics"))]
pub use mbus_server::ServerDiagnosticsHandler;
#[cfg(all(feature = "server", feature = "coils"))]
pub use mbus_server::{CoilMap, CoilsModel, ServerCoilHandler};
#[cfg(all(feature = "server", feature = "discrete-inputs"))]
pub use mbus_server::{DiscreteInputMap, DiscreteInputsModel, ServerDiscreteInputHandler};
#[cfg(all(feature = "server", feature = "fifo"))]
pub use mbus_server::{FifoQueueMap, ServerFifoHandler};
#[cfg(all(feature = "server", feature = "file-record"))]
pub use mbus_server::{FileRecordMap, ServerFileRecordHandler};
#[cfg(all(feature = "server", feature = "holding-registers"))]
pub use mbus_server::{HoldingRegisterMap, HoldingRegistersModel, ServerHoldingRegisterHandler};
#[cfg(all(feature = "server", feature = "input-registers"))]
pub use mbus_server::{InputRegisterMap, InputRegistersModel, ServerInputRegisterHandler};

#[cfg(feature = "client")]
pub use mbus_client::app::*;
#[cfg(feature = "client")]
pub use mbus_client::services::{ClientServices, SerialClientServices};
#[cfg(all(feature = "client", feature = "coils"))]
pub use mbus_client::services::coil::{Coils, MAX_COIL_BYTES, MAX_COILS_PER_PDU};
#[cfg(all(feature = "client", feature = "discrete-inputs"))]
pub use mbus_client::services::discrete_input::{
    DiscreteInputs, MAX_DISCRETE_INPUT_BYTES, MAX_DISCRETE_INPUTS_PER_PDU,
};
#[cfg(all(
    feature = "client",
    any(feature = "holding-registers", feature = "input-registers")
))]
pub use mbus_client::services::register::MAX_REGISTERS_PER_PDU;
#[cfg(all(feature = "client", feature = "holding-registers"))]
pub use mbus_client::services::register::Registers as HoldingRegisters;
#[cfg(all(feature = "client", feature = "input-registers"))]
pub use mbus_client::services::register::Registers as InputRegisters;
#[cfg(all(feature = "client", feature = "fifo"))]
pub use mbus_client::services::fifo_queue::{FifoQueue, MAX_FIFO_QUEUE_COUNT_PER_PDU};
#[cfg(all(feature = "client", feature = "file-record"))]
pub use mbus_client::services::file_record::{
    FILE_RECORD_REF_TYPE, MAX_SUB_REQUESTS_PER_PDU, SUB_REQ_PARAM_BYTE_LEN, SubRequest,
    SubRequestParams,
};
#[cfg(all(feature = "client", feature = "diagnostics"))]
pub use mbus_client::services::diagnostic::{
    BasicObjectId, ConformityLevel, DeviceIdObject, DeviceIdObjectIterator,
    DeviceIdentificationResponse, ExtendedObjectId, ObjectId, ReadDeviceIdCode, RegularObjectId,
};

#[cfg(feature = "async")]
pub use mbus_async;

pub use mbus_server_async;

#[cfg(feature = "gateway")]
pub use mbus_gateway;
