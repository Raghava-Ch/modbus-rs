//! Shared option/response structs for the Node.js (napi-rs) target.
#![cfg(not(target_arch = "wasm32"))]

use napi::bindgen_prelude::{Object, Uint16Array};
use napi_derive::napi;

/// Represents a Modbus coil or discrete input state.
#[napi]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CoilState {
    /// Coil is OFF (wire value: 0).
    Off = 0,
    /// Coil is ON (wire value: 1).
    On = 1,
}

// ── Request option structs ────────────────────────────────────────────────────

/// Connection options for the TCP transport.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct TcpTransportOptions {
    /// Target host address (IP or hostname).
    pub host: String,
    /// Target TCP port (typically 502).
    pub port: u16,
    /// Per-request timeout in milliseconds. Note: This feature is currently ineffective and is reserved for future implementation. A GitHub RFC discussion is open to decide whether to implement or remove it.
    pub request_timeout_ms: Option<u32>,
    /// Number of retry attempts on failure. Default: 0 (no retries).
    pub retry_attempts: Option<u32>,
    /// Delay between retry attempts in milliseconds.
    pub retry_delay_ms: Option<u32>,
    /// Backoff strategy for retries: "immediate", "fixed", or "exponential". Default: "immediate". Note: This feature is currently ineffective and is reserved for future implementation. A GitHub RFC discussion is open to decide whether to implement or remove it.
    pub retry_backoff_strategy: Option<String>,
}

/// Options for creating a client bound to a specific unit ID.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct CreateClientOptions {
    /// Modbus unit ID (slave address) from 1 to 247.
    pub unit_id: u8,
}

/// Options for reading registers.
#[napi(object)]
pub struct ReadRegistersOptions<'a> {
    /// Starting register address.
    pub address: u16,
    /// Number of registers to read (quantity).
    pub quantity: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing a single register.
#[napi(object)]
pub struct WriteSingleRegisterOptions<'a> {
    /// The register address.
    pub address: u16,
    /// The 16-bit value to write.
    pub value: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing multiple registers.
#[napi(object)]
pub struct WriteMultipleRegistersOptions<'a> {
    /// The starting register address.
    pub address: u16,
    /// An array of 16-bit values to write.
    pub values: Uint16Array,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for read/write multiple registers (FC23).
#[napi(object)]
pub struct ReadWriteMultipleRegistersOptions<'a> {
    /// Starting address for read operation.
    pub read_address: u16,
    /// Number of registers to read (quantity).
    pub read_quantity: u16,
    /// Starting address for write operation.
    pub write_address: u16,
    /// An array of 16-bit values to write.
    pub write_values: Uint16Array,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for reading coils or discrete inputs.
#[napi(object)]
pub struct ReadBitsOptions<'a> {
    /// Starting address.
    pub address: u16,
    /// Number of bits (coils or discrete inputs) to read.
    pub quantity: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing a single coil.
#[napi(object)]
pub struct WriteSingleCoilOptions<'a> {
    /// The coil address.
    pub address: u16,
    /// The coil state (CoilState.On = 1, CoilState.Off = 0).
    pub value: CoilState,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing multiple coils.
#[napi(object)]
pub struct WriteMultipleCoilsOptions<'a> {
    /// Starting coil address.
    pub address: u16,
    /// An array of coil states to write.
    pub values: Vec<CoilState>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for modifying a register with a bitwise mask (FC22).
#[napi(object)]
pub struct MaskWriteRegisterOptions<'a> {
    /// Target register address.
    pub address: u16,
    /// Bitwise AND mask.
    pub and_mask: u16,
    /// Bitwise OR mask.
    pub or_mask: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for reading FIFO queue.
#[napi(object)]
pub struct ReadFifoQueueOptions<'a> {
    /// FIFO pointer address.
    pub address: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// A single file record read sub-request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileRecordReadRequest {
    /// The file number (1-65535).
    pub file_number: u16,
    /// The starting record number within the file.
    pub record_number: u16,
    /// The number of records to read.
    pub record_length: u16,
}

/// Options for reading file records.
#[napi(object)]
pub struct ReadFileRecordOptions<'a> {
    /// An array of file record read sub-requests.
    pub requests: Vec<FileRecordReadRequest>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// A single file record write sub-request.
#[napi(object)]
pub struct FileRecordWriteRequest {
    /// The file number (1-65535).
    pub file_number: u16,
    /// The starting record number within the file.
    pub record_number: u16,
    /// The record data to write, as an array of 16-bit values.
    pub record_data: Uint16Array,
}

/// Options for writing file records.
#[napi(object)]
pub struct WriteFileRecordOptions<'a> {
    /// An array of file record write sub-requests.
    pub requests: Vec<FileRecordWriteRequest>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for device identification.
#[napi(object)]
pub struct ReadDeviceIdentificationOptions<'a> {
    /// Read device ID code (1=basic, 2=regular, 3=extended, 4=individual).
    pub read_device_id_code: Option<u8>,
    /// The ID of the first object to read.
    pub object_id: Option<u8>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for diagnostics request.
#[napi(object)]
pub struct DiagnosticsOptions<'a> {
    /// Diagnostic sub-function code.
    pub sub_function: u16,
    /// Data to be sent with the diagnostics request.
    pub data: Option<Uint16Array>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

// ── Response structs ──────────────────────────────────────────────────────────

/// Response from reading FIFO queue.
#[napi(object)]
pub struct FifoQueueResponse {
    /// The number of 16-bit values in the queue.
    pub count: u16,
    /// Queue values.
    pub values: Uint16Array,
}

/// Response from diagnostics request.
#[napi(object)]
pub struct DiagnosticsResponse {
    /// The sub-function code from the request.
    pub sub_function: u16,
    /// The data returned by the diagnostics function.
    pub data: Uint16Array,
}

/// Device identification object.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DeviceIdentificationObject {
    /// The object ID.
    pub id: u8,
    /// The value of the object, represented as a string.
    pub value: String,
}

/// Response from device identification request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DeviceIdentificationResponse {
    /// The conformity level of the device.
    pub conformity_level: u8,
    /// Indicates if more objects are available to be read.
    pub more_follows: bool,
    /// The ID of the next object to read if moreFollows is true.
    pub next_object_id: u8,
    /// An array of device identification objects.
    pub objects: Vec<DeviceIdentificationObject>,
}

/// Standard Modbus exception codes.
#[napi]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModbusExceptionCode {
    /// Function code received in the query is not an allowable action for the server.
    IllegalFunction = 1,
    /// The data address received in the query is not an allowable address for the server.
    IllegalDataAddress = 2,
    /// A value contained in the query data field is not an allowable value for the server.
    IllegalDataValue = 3,
    /// An unrecoverable error occurred while the server was attempting to perform the requested action.
    ServerDeviceFailure = 4,
    /// The server has accepted the request and is processing it, but a long duration of time will be required.
    Acknowledge = 5,
    /// The server is engaged in processing a long-duration program command.
    SlaveDeviceBusy = 6,
    /// The server attempted to read record file, but detected a parity error in the memory.
    MemoryParityError = 8,
    /// Specialized for gateways: The gateway was unable to allocate an internal communication path.
    GatewayPathUnavailable = 10,
    /// Specialized for gateways: No response was received from the target device.
    GatewayTargetDeviceFailedToRespond = 11,
}

/// Request parameters for reading device identification (MEI FC43/14).
#[napi(object)]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReadDeviceIdentificationRequest {
    /// Target device address.
    pub unit_id: u8,
    /// Read device ID code (1, 2, 3, or 4).
    pub read_device_id_code: u8,
    /// Starting object ID (0 to 255).
    pub object_id: u8,
}
