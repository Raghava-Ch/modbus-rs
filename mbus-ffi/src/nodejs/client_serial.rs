//! Node.js bindings for the async Modbus Serial client (RTU/ASCII).

use std::sync::{Arc, Mutex};
use std::time::Duration;

use napi::bindgen_prelude::*;
use napi_derive::napi;

use mbus_client_async::AsyncSerialClient;
use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
    UnitIdOrSlaveAddr,
};

#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};
#[cfg(feature = "file-record")]
use mbus_client_async::SubRequest;

use crate::nodejs::client_tcp::{
    DeviceIdentificationObject, DeviceIdentificationResponse, DiagnosticsOptions,
    DiagnosticsResponse, ReadBitsOptions, ReadDeviceIdentificationOptions, ReadFifoQueueOptions,
    ReadFileRecordOptions, ReadRegistersOptions, ReadWriteMultipleRegistersOptions,
    WriteFileRecordOptions, WriteMultipleCoilsOptions, WriteMultipleRegistersOptions,
    WriteSingleCoilOptions, WriteSingleRegisterOptions,
};
use crate::nodejs::errors::{from_async_error, to_napi_err, ERR_MODBUS_INVALID_ARGUMENT};

// ── Option structs ───────────────────────────────────────────────────────────

/// Connection options for the serial client.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SerialClientOptions {
    /// Serial port path (e.g., "/dev/ttyUSB0", "COM3").
    pub port_path: String,
    /// Baud rate (e.g., 9600, 19200, 38400, 57600, 115200).
    pub baud_rate: u32,
    /// Data bits (5, 6, 7, or 8).
    pub data_bits: Option<u8>,
    /// Parity ("none", "even", "odd").
    pub parity: Option<String>,
    /// Stop bits (1 or 2).
    pub stop_bits: Option<u8>,
    /// Modbus unit ID (1-247).
    pub unit_id: u8,
    /// Response timeout in milliseconds.
    pub response_timeout_ms: Option<u32>,
    /// Per-request timeout in milliseconds.
    pub request_timeout_ms: Option<u32>,
}

/// Converts a string parity value to the Parity enum.
fn parse_parity(s: &str) -> Result<Parity> {
    match s.to_lowercase().as_str() {
        "none" | "n" => Ok(Parity::None),
        "even" | "e" => Ok(Parity::Even),
        "odd" | "o" => Ok(Parity::Odd),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!("Invalid parity value: '{}'. Expected 'none', 'even', or 'odd'", s),
        )),
    }
}

/// Converts a numeric baud rate to BaudRate enum.
fn parse_baud_rate(rate: u32) -> Result<BaudRate> {
    match rate {
        9600 => Ok(BaudRate::Baud9600),
        19200 => Ok(BaudRate::Baud19200),
        _ => Ok(BaudRate::Custom(rate)),
    }
}

/// Converts numeric data bits to DataBits enum.
fn parse_data_bits(bits: u8) -> Result<DataBits> {
    match bits {
        5 => Ok(DataBits::Five),
        6 => Ok(DataBits::Six),
        7 => Ok(DataBits::Seven),
        8 => Ok(DataBits::Eight),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!("Invalid data bits: {}. Expected 5, 6, 7, or 8", bits),
        )),
    }
}

/// Converts numeric stop bits to u8 (validated).
fn parse_stop_bits(bits: u8) -> Result<u8> {
    match bits {
        1 | 2 => Ok(bits),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!("Invalid stop bits: {}. Expected 1 or 2", bits),
        )),
    }
}

/// Builds a ModbusSerialConfig from options with defaults.
fn build_serial_config(opts: &SerialClientOptions, mode: SerialMode) -> Result<ModbusSerialConfig> {
    let baud_rate = parse_baud_rate(opts.baud_rate)?;
    let data_bits = opts.data_bits.map(parse_data_bits).transpose()?.unwrap_or(DataBits::Eight);
    let parity = opts
        .parity
        .as_ref()
        .map(|s| parse_parity(s))
        .transpose()?
        .unwrap_or(Parity::None);
    let stop_bits = opts.stop_bits.map(parse_stop_bits).transpose()?.unwrap_or(1);
    let response_timeout_ms = opts.response_timeout_ms.unwrap_or(1000);

    let port_path = heapless::String::try_from(opts.port_path.as_str())
        .map_err(|_| napi::Error::new(Status::InvalidArg, "Port path too long (max 64 chars)"))?;

    Ok(ModbusSerialConfig {
        port_path,
        mode,
        baud_rate,
        data_bits,
        stop_bits,
        parity,
        response_timeout_ms,
        retry_attempts: 0,
        retry_backoff_strategy: BackoffStrategy::Immediate,
        retry_jitter_strategy: JitterStrategy::None,
        retry_random_fn: None,
    })
}

// ── AsyncSerialModbusClient ──────────────────────────────────────────────────

/// Async Modbus Serial client supporting RTU and ASCII transports.
#[napi]
pub struct AsyncSerialModbusClient {
    inner: Mutex<Option<Arc<AsyncSerialClient>>>,
    unit_id: u8,
}

#[napi]
impl AsyncSerialModbusClient {
    /// Creates and connects a new Serial RTU client.
    #[napi(factory)]
    pub async fn connect_rtu(opts: SerialClientOptions) -> Result<AsyncSerialModbusClient> {
        UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let config = build_serial_config(&opts, SerialMode::Rtu)?;

        let client = AsyncSerialClient::new_rtu(config)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        client.connect().await.map_err(from_async_error)?;

        if let Some(timeout_ms) = opts.request_timeout_ms {
            client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        }

        Ok(AsyncSerialModbusClient {
            inner: Mutex::new(Some(Arc::new(client))),
            unit_id: opts.unit_id,
        })
    }

    /// Creates and connects a new Serial ASCII client.
    #[napi(factory)]
    pub async fn connect_ascii(opts: SerialClientOptions) -> Result<AsyncSerialModbusClient> {
        UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let config = build_serial_config(&opts, SerialMode::Ascii)?;

        let client = AsyncSerialClient::new_ascii(config)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        client.connect().await.map_err(from_async_error)?;

        if let Some(timeout_ms) = opts.request_timeout_ms {
            client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        }

        Ok(AsyncSerialModbusClient {
            inner: Mutex::new(Some(Arc::new(client))),
            unit_id: opts.unit_id,
        })
    }

    fn get_client(&self) -> Result<Arc<AsyncSerialClient>> {
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

        let mut output: Vec<Vec<u16>> = Vec::new();
        for params in result {
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

        let mut sub_request = SubRequest::new();
        for req in &opts.requests {
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
