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
#[cfg(feature = "client")]
pub use mbus_core::transport::{BackoffStrategy, JitterStrategy};
#[cfg(any(feature = "client", feature = "server"))]
pub use mbus_core::transport::{
    BaudRate, DataBits, ModbusConfig, ModbusSerialConfig, ModbusTcpConfig, Parity, SerialMode,
    TimeKeeper, Transport, TransportError, TransportType, UnitIdOrSlaveAddr, checksum::crc16,
};

#[cfg(all(feature = "network-tcp", feature = "async"))]
pub use mbus_network::TokioTcpTransport;
#[cfg(feature = "network-tcp")]
pub use mbus_network::{StdTcpServerTransport, StdTcpTransport};
#[cfg(all(feature = "serial-rtu", feature = "async"))]
pub use mbus_serial::TokioRtuTransport;
#[cfg(all(feature = "serial-ascii", feature = "async"))]
pub use mbus_serial::TokioAsciiTransport;
#[cfg(feature = "serial-ascii")]
pub use mbus_serial::StdAsciiTransport;
#[cfg(feature = "serial-rtu")]
pub use mbus_serial::StdRtuTransport;
#[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
pub use mbus_serial::StdSerialTransport;

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
#[cfg(feature = "coils")]
pub use mbus_core::models::coil::{Coils, MAX_COIL_BYTES, MAX_COILS_PER_PDU};
#[cfg(feature = "diagnostics")]
pub use mbus_core::models::diagnostic::{
    BasicObjectId, ConformityLevel, DeviceIdObject, DeviceIdObjectIterator,
    DeviceIdentificationResponse, ExtendedObjectId, ObjectId, ReadDeviceIdCode, RegularObjectId,
};
#[cfg(feature = "discrete-inputs")]
pub use mbus_core::models::discrete_input::{
    DiscreteInputs, MAX_DISCRETE_INPUT_BYTES, MAX_DISCRETE_INPUTS_PER_PDU,
};
#[cfg(feature = "fifo")]
pub use mbus_core::models::fifo_queue::{FifoQueue, MAX_FIFO_QUEUE_COUNT_PER_PDU};
#[cfg(feature = "file-record")]
pub use mbus_core::models::file_record::{
    FILE_RECORD_REF_TYPE, MAX_SUB_REQUESTS_PER_PDU, SUB_REQ_PARAM_BYTE_LEN, SubRequest,
    SubRequestParams,
};
#[cfg(feature = "holding-registers")]
pub use mbus_core::models::register::HoldingRegisters;
#[cfg(feature = "input-registers")]
pub use mbus_core::models::register::InputRegisters;
#[cfg(any(feature = "holding-registers", feature = "input-registers"))]
#[allow(deprecated)]
pub use mbus_core::models::register::{MAX_REGISTERS_PER_PDU, Registers};

#[cfg(all(any(feature = "server", feature = "client"), feature = "async"))]
#[deprecated(
    since = "0.10.0",
    note = "The `mbus-async` crate is obsolete and consolidated into `mbus_server_async` and `mbus_client_async`. mbus-async will be removed in the near future. Please migrate to using `mbus_server_async` and `mbus_client_async` directly."
)]
pub use mbus_async;

#[cfg(all(feature = "server", feature = "async"))]
pub use mbus_server_async;

#[cfg(all(feature = "client", feature = "async", feature = "traffic"))]
pub use mbus_client_async::AsyncClientTrafficNotifier;
#[cfg(all(feature = "server", feature = "async", feature = "traffic"))]
pub use mbus_server_async::AsyncServerTrafficNotifier;

#[cfg(all(feature = "server", feature = "serial-ascii", feature = "async"))]
pub use mbus_server_async::AsyncAsciiServer;
#[cfg(all(feature = "server", feature = "serial-rtu", feature = "async"))]
pub use mbus_server_async::AsyncRtuServer;
#[cfg(all(feature = "server", feature = "network-tcp", feature = "async"))]
pub use mbus_server_async::AsyncTcpServer;
#[cfg(all(feature = "server", feature = "async"))]
pub use mbus_server_async::{AsyncAppHandler, async_modbus_app};

#[cfg(all(feature = "client", feature = "serial-ascii", feature = "async"))]
pub use mbus_client_async::AsyncAsciiClient;
#[cfg(all(feature = "client", feature = "serial-rtu", feature = "async"))]
pub use mbus_client_async::AsyncRtuClient;
#[cfg(all(
    feature = "client",
    any(feature = "serial-rtu", feature = "serial-ascii"),
    feature = "async"
))]
pub use mbus_client_async::AsyncSerialClientKind;
#[cfg(all(feature = "client", feature = "network-tcp", feature = "async"))]
pub use mbus_client_async::AsyncTcpClient;

#[cfg(feature = "gateway")]
pub use mbus_gateway as gateway;
