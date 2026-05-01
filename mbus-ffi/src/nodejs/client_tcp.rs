//! Node.js bindings for the async Modbus TCP client.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use mbus_client_async::AsyncTcpClient;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_client_async::SubRequest;

use crate::nodejs::errors::{from_async_error, to_napi_err, ERR_MODBUS_INVALID_ARGUMENT};

// ── Option structs ───────────────────────────────────────────────────────────

/// Connection options for the TCP client.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct TcpClientOptions {
    /// Target host address (IP or hostname).
    pub host: String,
    /// Target TCP port (typically 502).
    pub port: u16,
    /// Modbus unit ID (1-247).
    pub unit_id: u8,
    /// Per-request timeout in milliseconds (optional).
    pub timeout_ms: Option<u32>,
}

/// Options for reading registers.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadRegistersOptions {
    /// Starting register address.
    pub address: u16,
    /// Number of registers to read.
    pub quantity: u16,
}

/// Options for writing a single register.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteSingleRegisterOptions {
    /// Register address.
    pub address: u16,
    /// Value to write.
    pub value: u16,
}

/// Options for writing multiple registers.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteMultipleRegistersOptions {
    /// Starting register address.
    pub address: u16,
    /// Values to write.
    pub values: Vec<u16>,
}

/// Options for read/write multiple registers (FC23).
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadWriteMultipleRegistersOptions {
    /// Starting address for read operation.
    pub read_address: u16,
    /// Number of registers to read.
    pub read_quantity: u16,
    /// Starting address for write operation.
    pub write_address: u16,
    /// Values to write.
    pub write_values: Vec<u16>,
}

/// Options for reading coils or discrete inputs.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadBitsOptions {
    /// Starting address.
    pub address: u16,
    /// Number of bits to read.
    pub quantity: u16,
}

/// Options for writing a single coil.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteSingleCoilOptions {
    /// Coil address.
    pub address: u16,
    /// Value to write.
    pub value: bool,
}

/// Options for writing multiple coils.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteMultipleCoilsOptions {
    /// Starting coil address.
    pub address: u16,
    /// Values to write.
    pub values: Vec<bool>,
}

/// Options for reading FIFO queue.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadFifoQueueOptions {
    /// FIFO pointer address.
    pub address: u16,
}

/// A single file record read sub-request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileRecordReadRequest {
    /// File number (1-65535).
    pub file_number: u16,
    /// Starting record number.
    pub record_number: u16,
    /// Number of records to read.
    pub record_length: u16,
}

/// Options for reading file records.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadFileRecordOptions {
    /// Array of sub-requests.
    pub requests: Vec<FileRecordReadRequest>,
}

/// A single file record write sub-request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FileRecordWriteRequest {
    /// File number (1-65535).
    pub file_number: u16,
    /// Starting record number.
    pub record_number: u16,
    /// Record data to write.
    pub record_data: Vec<u16>,
}

/// Options for writing file records.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct WriteFileRecordOptions {
    /// Array of sub-requests.
    pub requests: Vec<FileRecordWriteRequest>,
}

/// Options for device identification.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct ReadDeviceIdentificationOptions {
    /// Read device ID code (1=basic, 2=regular, 3=extended, 4=individual).
    pub read_device_id_code: u8,
    /// Starting object ID.
    pub object_id: u8,
}

/// Options for diagnostics request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DiagnosticsOptions {
    /// Diagnostic sub-function code.
    pub sub_function: u16,
    /// Data words for the request.
    pub data: Vec<u16>,
}

/// Response from diagnostics request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DiagnosticsResponse {
    /// Echoed sub-function code.
    pub sub_function: u16,
    /// Response data words.
    pub data: Vec<u16>,
}

/// Device identification object.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DeviceIdentificationObject {
    /// Object ID.
    pub id: u8,
    /// Object value as string.
    pub value: String,
}

/// Response from device identification request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DeviceIdentificationResponse {
    /// Conformity level.
    pub conformity_level: u8,
    /// Whether more objects follow.
    pub more_follows: bool,
    /// Next object ID if more follows.
    pub next_object_id: u8,
    /// Retrieved objects.
    pub objects: Vec<DeviceIdentificationObject>,
}

// ── AsyncTcpModbusClient ─────────────────────────────────────────────────────

/// Async Modbus TCP client.
///
/// All methods are async and return Promises. The client must be connected
/// before issuing Modbus requests.
#[napi]
pub struct AsyncTcpModbusClient {
    inner: Mutex<Option<Arc<AsyncTcpClient>>>,
    unit_id: u8,
}

#[napi]
impl AsyncTcpModbusClient {
    /// Creates and connects a new TCP client.
    ///
    /// @param opts - Connection options including host, port, unitId, and optional timeout.
    /// @returns A connected client instance.
    #[napi(factory)]
    pub async fn connect(opts: TcpClientOptions) -> Result<AsyncTcpModbusClient> {
        // Validate unit ID
        UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let client = AsyncTcpClient::new(&opts.host, opts.port)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        client.connect().await.map_err(from_async_error)?;

        if let Some(timeout_ms) = opts.timeout_ms {
            client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        }

        Ok(AsyncTcpModbusClient {
            inner: Mutex::new(Some(Arc::new(client))),
            unit_id: opts.unit_id,
        })
    }

    fn get_client(&self) -> Result<Arc<AsyncTcpClient>> {
        let guard = self.inner.lock().map_err(|_| {
            napi::Error::new(Status::GenericFailure, "Failed to acquire lock")
        })?;
        guard
            .clone()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "Client is closed"))
    }

    /// Sets the per-request timeout in milliseconds.
    #[napi]
    pub fn set_request_timeout(&self, timeout_ms: u32) -> Result<()> {
        let client = self.get_client()?;
        client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        Ok(())
    }

    /// Clears the per-request timeout.
    #[napi]
    pub fn clear_request_timeout(&self) -> Result<()> {
        let client = self.get_client()?;
        client.clear_request_timeout();
        Ok(())
    }

    /// Returns whether there are pending requests.
    #[napi(getter)]
    pub fn pending_requests(&self) -> Result<bool> {
        let client = self.get_client()?;
        Ok(client.has_pending_requests())
    }

    /// Closes the client connection.
    #[napi]
    pub async fn close(&self) -> Result<()> {
        let client = {
            let mut guard = self.inner.lock().map_err(|_| {
                napi::Error::new(Status::GenericFailure, "Failed to acquire lock")
            })?;
            guard.take()
        };
        if let Some(inner) = client {
            let _ = inner.disconnect().await;
        }
        Ok(())
    }

    /// Reconnects the client after a disconnect.
    #[napi]
    pub async fn reconnect(&self) -> Result<()> {
        let client = self.get_client()?;
        client.connect().await.map_err(from_async_error)
    }

    // ── Register methods ─────────────────────────────────────────────────────

    /// Reads holding registers (FC03).
    #[napi]
    #[cfg(feature = "registers")]
    pub async fn read_holding_registers(&self, opts: ReadRegistersOptions) -> Result<Vec<u16>> {
        let client = self.get_client()?;
        let regs = client
            .read_holding_registers(self.unit_id, opts.address, opts.quantity)
            .await
            .map_err(from_async_error)?;
        Ok(regs.values()[..regs.quantity() as usize].to_vec())
    }

    /// Reads input registers (FC04).
    #[napi]
    #[cfg(feature = "registers")]
    pub async fn read_input_registers(&self, opts: ReadRegistersOptions) -> Result<Vec<u16>> {
        let client = self.get_client()?;
        let regs = client
            .read_input_registers(self.unit_id, opts.address, opts.quantity)
            .await
            .map_err(from_async_error)?;
        Ok(regs.values()[..regs.quantity() as usize].to_vec())
    }

    /// Writes a single register (FC06).
    #[napi]
    #[cfg(feature = "registers")]
    pub async fn write_single_register(&self, opts: WriteSingleRegisterOptions) -> Result<()> {
        let client = self.get_client()?;
        client
            .write_single_register(self.unit_id, opts.address, opts.value)
            .await
            .map_err(from_async_error)?;
        Ok(())
    }

    /// Writes multiple registers (FC16).
    #[napi]
    #[cfg(feature = "registers")]
    pub async fn write_multiple_registers(&self, opts: WriteMultipleRegistersOptions) -> Result<()> {
        let client = self.get_client()?;
        client
            .write_multiple_registers(self.unit_id, opts.address, &opts.values)
            .await
            .map_err(from_async_error)?;
        Ok(())
    }

    /// Reads and writes multiple registers atomically (FC23).
    #[napi]
    #[cfg(feature = "registers")]
    pub async fn read_write_multiple_registers(
        &self,
        opts: ReadWriteMultipleRegistersOptions,
    ) -> Result<Vec<u16>> {
        let client = self.get_client()?;
        let regs = client
            .read_write_multiple_registers(
                self.unit_id,
                opts.read_address,
                opts.read_quantity,
                opts.write_address,
                &opts.write_values,
            )
            .await
            .map_err(from_async_error)?;
        Ok(regs.values()[..regs.quantity() as usize].to_vec())
    }

    // ── Coil methods ─────────────────────────────────────────────────────────

    /// Reads coils (FC01).
    #[napi]
    #[cfg(feature = "coils")]
    pub async fn read_coils(&self, opts: ReadBitsOptions) -> Result<Vec<bool>> {
        let client = self.get_client()?;
        let coils = client
            .read_multiple_coils(self.unit_id, opts.address, opts.quantity)
            .await
            .map_err(from_async_error)?;

        let mut result = Vec::with_capacity(opts.quantity as usize);
        for i in 0..opts.quantity {
            result.push(coils.value(opts.address + i).unwrap_or(false));
        }
        Ok(result)
    }

    /// Writes a single coil (FC05).
    #[napi]
    #[cfg(feature = "coils")]
    pub async fn write_single_coil(&self, opts: WriteSingleCoilOptions) -> Result<()> {
        let client = self.get_client()?;
        client
            .write_single_coil(self.unit_id, opts.address, opts.value)
            .await
            .map_err(from_async_error)?;
        Ok(())
    }

    /// Writes multiple coils (FC15).
    #[napi]
    #[cfg(feature = "coils")]
    pub async fn write_multiple_coils(&self, opts: WriteMultipleCoilsOptions) -> Result<()> {
        use mbus_core::models::coil::Coils;

        let client = self.get_client()?;

        // Build Coils from bool array
        let qty = opts.values.len() as u16;
        let mut coils = Coils::new(opts.address, qty)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        for (i, &value) in opts.values.iter().enumerate() {
            coils
                .set_value(opts.address + i as u16, value)
                .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;
        }

        client
            .write_multiple_coils(self.unit_id, opts.address, &coils)
            .await
            .map_err(from_async_error)?;
        Ok(())
    }

    // ── Discrete inputs ──────────────────────────────────────────────────────

    /// Reads discrete inputs (FC02).
    #[napi]
    #[cfg(feature = "discrete-inputs")]
    pub async fn read_discrete_inputs(&self, opts: ReadBitsOptions) -> Result<Vec<bool>> {
        let client = self.get_client()?;
        let inputs = client
            .read_discrete_inputs(self.unit_id, opts.address, opts.quantity)
            .await
            .map_err(from_async_error)?;

        let mut result = Vec::with_capacity(opts.quantity as usize);
        for i in 0..opts.quantity {
            result.push(inputs.value(opts.address + i).unwrap_or(false));
        }
        Ok(result)
    }

    // ── FIFO ─────────────────────────────────────────────────────────────────

    /// Reads FIFO queue (FC24).
    #[napi]
    #[cfg(feature = "fifo")]
    pub async fn read_fifo_queue(&self, opts: ReadFifoQueueOptions) -> Result<Vec<u16>> {
        let client = self.get_client()?;
        let fifo = client
            .read_fifo_queue(self.unit_id, opts.address)
            .await
            .map_err(from_async_error)?;
        Ok(fifo.queue().to_vec())
    }

    // ── File record ──────────────────────────────────────────────────────────

    /// Reads file records (FC20).
    #[napi]
    #[cfg(feature = "file-record")]
    pub async fn read_file_record(&self, opts: ReadFileRecordOptions) -> Result<Vec<Vec<u16>>> {
        let client = self.get_client()?;

        // Build SubRequest from options
        let mut sub_request = SubRequest::new();
        for req in &opts.requests {
            sub_request
                .add_read_sub_request(req.file_number, req.record_number, req.record_length)
                .map_err(|e| {
                    napi::Error::new(Status::InvalidArg, format!("File record error: {:?}", e))
                })?;
        }

        let result = client
            .read_file_record(self.unit_id, &sub_request)
            .await
            .map_err(from_async_error)?;

        // Convert each sub-response to Vec<u16>
        let mut output: Vec<Vec<u16>> = Vec::new();
        for params in result {
            // record_data is Option<heapless::Vec<u16, ...>>
            let words: Vec<u16> = params
                .record_data
                .map(|v| v.into_iter().collect())
                .unwrap_or_default();
            output.push(words);
        }
        Ok(output)
    }

    /// Writes file records (FC21).
    #[napi]
    #[cfg(feature = "file-record")]
    pub async fn write_file_record(&self, opts: WriteFileRecordOptions) -> Result<()> {
        use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;

        let client = self.get_client()?;

        // Build SubRequest with data
        let mut sub_request = SubRequest::new();
        for req in &opts.requests {
            // Convert Vec<u16> to heapless::Vec
            let record_data: heapless::Vec<u16, MAX_PDU_DATA_LEN> =
                heapless::Vec::from_slice(&req.record_data)
                    .map_err(|_| napi::Error::new(Status::InvalidArg, "Record data too large"))?;

            sub_request
                .add_write_sub_request(
                    req.file_number,
                    req.record_number,
                    req.record_data.len() as u16,
                    record_data,
                )
                .map_err(|e| {
                    napi::Error::new(Status::InvalidArg, format!("File record error: {:?}", e))
                })?;
        }

        client
            .write_file_record(self.unit_id, &sub_request)
            .await
            .map_err(from_async_error)?;
        Ok(())
    }

    // ── Diagnostics ──────────────────────────────────────────────────────────

    /// Reads exception status (FC07).
    #[napi]
    #[cfg(feature = "diagnostics")]
    pub async fn read_exception_status(&self) -> Result<u8> {
        let client = self.get_client()?;
        client
            .read_exception_status(self.unit_id)
            .await
            .map_err(from_async_error)
    }

    /// Sends a diagnostics request (FC08).
    #[napi]
    #[cfg(feature = "diagnostics")]
    pub async fn diagnostics(&self, opts: DiagnosticsOptions) -> Result<DiagnosticsResponse> {
        let client = self.get_client()?;

        let sub_function = DiagnosticSubFunction::try_from(opts.sub_function).map_err(|_| {
            napi::Error::new(
                Status::InvalidArg,
                format!("Invalid diagnostic sub-function code: {}", opts.sub_function),
            )
        })?;

        let result = client
            .diagnostics(self.unit_id, sub_function, &opts.data)
            .await
            .map_err(from_async_error)?;

        Ok(DiagnosticsResponse {
            sub_function: u16::from(result.sub_function),
            data: result.data,
        })
    }

    /// Reads device identification (FC43/MEI14).
    #[napi]
    #[cfg(feature = "diagnostics")]
    pub async fn read_device_identification(
        &self,
        opts: ReadDeviceIdentificationOptions,
    ) -> Result<DeviceIdentificationResponse> {
        use mbus_core::models::diagnostic::ConformityLevel;

        let client = self.get_client()?;

        let read_device_id_code = ReadDeviceIdCode::try_from(opts.read_device_id_code)
            .map_err(|_| {
                napi::Error::new(
                    Status::InvalidArg,
                    format!("Invalid read device ID code: {}", opts.read_device_id_code),
                )
            })?;

        let object_id = ObjectId::from(opts.object_id);

        let result = client
            .read_device_identification(self.unit_id, read_device_id_code, object_id)
            .await
            .map_err(from_async_error)?;

        let objects: Vec<DeviceIdentificationObject> = result
            .objects()
            .filter_map(|obj_result| obj_result.ok())
            .map(|obj| DeviceIdentificationObject {
                id: u8::from(obj.object_id),
                value: String::from_utf8_lossy(&obj.value).to_string(),
            })
            .collect();

        // Convert ConformityLevel to u8
        let conformity_level: u8 = match result.conformity_level {
            ConformityLevel::BasicStreamOnly => 0x01,
            ConformityLevel::RegularStreamOnly => 0x02,
            ConformityLevel::ExtendedStreamOnly => 0x03,
            ConformityLevel::BasicStreamAndIndividual => 0x81,
            ConformityLevel::RegularStreamAndIndividual => 0x82,
            ConformityLevel::ExtendedStreamAndIndividual => 0x83,
        };

        Ok(DeviceIdentificationResponse {
            conformity_level,
            more_follows: result.more_follows,
            next_object_id: u8::from(result.next_object_id),
            objects,
        })
    }
}
