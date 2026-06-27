//! Node.js bindings for the async Modbus TCP client.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use napi::bindgen_prelude::*;
use napi_derive::napi;

type TcpClient = mbus_client_async::AsyncTcpClient<10000>;
use mbus_core::transport::UnitIdOrSlaveAddr;

#[cfg(feature = "file-record")]
use mbus_client_async::SubRequest;
#[cfg(feature = "diagnostics")]
use mbus_core::function_codes::public::DiagnosticSubFunction;
#[cfg(feature = "diagnostics")]
use mbus_core::models::diagnostic::{ObjectId, ReadDeviceIdCode};

use crate::nodejs::errors::{
    ERR_MODBUS_INVALID_ARGUMENT, from_async_error, parse_backoff_strategy, to_napi_err,
};

unsafe fn extend_lifetime<'a, 'b, T>(p: PromiseRaw<'a, T>) -> PromiseRaw<'b, T> {
    unsafe { std::mem::transmute(p) }
}

// ── Option structs ───────────────────────────────────────────────────────────

/// Connection options for the TCP transport.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct TcpTransportOptions {
    /// Target host address (IP or hostname).
    pub host: String,
    /// Target TCP port (typically 502).
    pub port: u16,
    /// Per-request timeout in milliseconds.
    pub request_timeout_ms: Option<u32>,
    /// Number of retry attempts on failure. Default: 0 (no retries).
    pub retry_attempts: Option<u32>,
    /// Delay between retry attempts in milliseconds.
    pub retry_delay_ms: Option<u32>,
    /// Backoff strategy for retries: "immediate", "fixed", or "exponential". Default: "immediate".
    pub retry_backoff_strategy: Option<String>,
}

/// Options for creating a device client.
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
    /// Register address.
    pub address: u16,
    /// The 16-bit value to write to the register.
    pub value: u16,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing multiple registers.
#[napi(object)]
pub struct WriteMultipleRegistersOptions<'a> {
    /// Starting register address.
    pub address: u16,
    /// An array of 16-bit values to write.
    pub values: Vec<u16>,
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
    pub write_values: Vec<u16>,
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
    /// Coil address.
    pub address: u16,
    /// The boolean value to write (true for ON, false for OFF).
    pub value: bool,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for writing multiple coils.
#[napi(object)]
pub struct WriteMultipleCoilsOptions<'a> {
    /// Starting coil address.
    pub address: u16,
    /// An array of boolean values to write.
    pub values: Vec<bool>,
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

/// Response from reading FIFO queue.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct FifoQueueResponse {
    /// The number of values returned from the FIFO queue.
    pub count: u16,
    /// The values from the FIFO queue as an array of 16-bit numbers.
    pub values: Vec<u16>,
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
#[derive(Debug, Clone)]
pub struct FileRecordWriteRequest {
    /// The file number (1-65535).
    pub file_number: u16,
    /// The starting record number within the file.
    pub record_number: u16,
    /// The record data to write, as an array of 16-bit values.
    pub record_data: Vec<u16>,
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
    pub read_device_id_code: u8,
    /// The ID of the first object to read.
    pub object_id: u8,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Options for diagnostics request.
#[napi(object)]
pub struct DiagnosticsOptions<'a> {
    /// Diagnostic sub-function code.
    pub sub_function: u16,
    /// Data to be sent with the diagnostics request.
    pub data: Vec<u16>,
    /// An optional `AbortSignal` to cancel the asynchronous operation.
    pub signal: Option<Object<'a>>,
}

/// Response from diagnostics request.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct DiagnosticsResponse {
    /// The sub-function code from the request.
    pub sub_function: u16,
    /// The data returned by the diagnostics function.
    pub data: Vec<u16>,
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
    /// The ID of the next object to read if `moreFollows` is true.
    pub next_object_id: u8,
    /// An array of device identification objects.
    pub objects: Vec<DeviceIdentificationObject>,
}

// ── AsyncTcpTransport ────────────────────────────────────────────────────────

/// Physical TCP socket connection to a Modbus device or gateway.
#[napi]
#[doc = "Manages a physical TCP socket connection to a Modbus device or gateway."]
#[doc = "This class is a factory for creating lightweight `AsyncTcpModbusClient` instances"]
#[doc = "that are bound to a specific unit ID and share the underlying TCP connection."]
#[doc = ""]
#[doc = "Use the static `connect` method to establish a connection."]
pub struct AsyncTcpTransport {
    inner: Mutex<Option<Arc<TcpClient>>>,
}

#[napi]
impl AsyncTcpTransport {
    /// Connects to a Modbus TCP device or gateway.
    #[napi(factory)]
    #[doc = "Establishes a connection to a Modbus TCP device or gateway."]
    #[doc = ""]
    #[doc = "@param opts - Connection options including host, port, and timeout settings."]
    #[doc = "@param opts.host - Target host address (IP or hostname)."]
    #[doc = "@param opts.port - Target TCP port (typically 502)."]
    pub async fn connect(opts: TcpTransportOptions) -> Result<AsyncTcpTransport> {
        if let Some(ref strategy) = opts.retry_backoff_strategy {
            let _ = parse_backoff_strategy(strategy)?;
        }

        let client = TcpClient::new_with_pipeline(&opts.host, opts.port)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        client.connect().await.map_err(from_async_error)?;

        if let Some(timeout_ms) = opts.request_timeout_ms {
            client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        }

        let retry_attempts = opts.retry_attempts.unwrap_or(0) as u8;
        let retry_delay = Duration::from_millis(opts.retry_delay_ms.unwrap_or(0) as u64);
        client.set_retry_options(retry_attempts, retry_delay);

        Ok(AsyncTcpTransport {
            inner: Mutex::new(Some(Arc::new(client))),
        })
    }

    fn get_client(&self) -> Result<Arc<TcpClient>> {
        let guard = self
            .inner
            .lock()
            .map_err(|_| napi::Error::new(Status::GenericFailure, "Failed to acquire lock"))?;
        guard
            .clone()
            .ok_or_else(|| napi::Error::new(Status::GenericFailure, "Transport is closed"))
    }

    /// Creates a device client bound to the specified unit ID.
    #[napi]
    #[doc = "Creates a lightweight device client that communicates through this transport."]
    #[doc = "The client is bound to a specific Modbus unit ID."]
    #[doc = ""]
    #[doc = "@param opts - Options specifying the unit ID."]
    pub fn create_client(&self, opts: CreateClientOptions) -> Result<AsyncTcpModbusClient> {
        let client = self.get_client()?;
        let unit_id = opts.unit_id;
        UnitIdOrSlaveAddr::new(unit_id).map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;
        Ok(AsyncTcpModbusClient {
            inner: client,
            unit_id,
        })
    }

    /// Sets the per-request timeout in milliseconds.
    #[napi]
    #[doc = "Sets the timeout for individual Modbus requests made through this transport."]
    #[doc = ""]
    #[doc = "@param timeout_ms - The timeout duration in milliseconds."]
    pub fn set_request_timeout(&self, timeout_ms: u32) -> Result<()> {
        let client = self.get_client()?;
        client.set_request_timeout(Duration::from_millis(timeout_ms as u64));
        Ok(())
    }

    /// Clears the per-request timeout.
    #[napi]
    #[doc = "Clears any previously set per-request timeout, reverting to the default behavior."]
    pub fn clear_request_timeout(&self) -> Result<()> {
        let client = self.get_client()?;
        client.clear_request_timeout();
        Ok(())
    }

    /// Returns whether there are pending requests.
    #[napi(getter)]
    #[doc = "Checks if there are any Modbus requests currently in-flight on this transport."]
    pub fn pending_requests(&self) -> Result<bool> {
        let client = self.get_client()?;
        Ok(client.has_pending_requests())
    }

    /// Closes the connection.
    #[doc = "Closes the underlying TCP connection and shuts down the transport."]
    #[napi]
    pub async fn close(&self) -> Result<()> {
        let client = {
            let mut guard = self
                .inner
                .lock()
                .map_err(|_| napi::Error::new(Status::GenericFailure, "Failed to acquire lock"))?;
            guard.take()
        };
        if let Some(inner) = client {
            let _ = inner.shutdown().await;
        }
        Ok(())
    }

    /// Reconnects the transport after a disconnect.
    #[napi]
    #[doc = "Attempts to re-establish the TCP connection if it has been closed."]
    pub async fn reconnect(&self) -> Result<()> {
        let client = self.get_client()?;
        client.connect().await.map_err(from_async_error)
    }
}

// ── AsyncTcpModbusClient ─────────────────────────────────────────────────────

/// Lightweight device client sharing the TCP transport.
#[doc = "A lightweight Modbus client that sends requests for a specific unit ID"]
#[doc = "over a shared `AsyncTcpTransport`."]
#[napi]
pub struct AsyncTcpModbusClient {
    inner: Arc<TcpClient>,
    unit_id: u8,
}

#[napi]
impl AsyncTcpModbusClient {
    // ── Register methods ─────────────────────────────────────────────────────

    /// Reads holding registers (FC03).
    #[napi(ts_return_type = "Promise<number[]>")]
    #[cfg(feature = "holding-registers")]
    #[doc = "Reads one or more holding registers (Function Code 03)."]
    #[doc = "@param opts Options for reading registers."]
    #[doc = "@param opts.address The starting register address."]
    #[doc = "@param opts.quantity The number of registers to read."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_holding_registers(
        &self,
        env: Env,
        opts: ReadRegistersOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<u16>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let quantity = opts.quantity;

        let promise = env.spawn_future(async move {
            let fut = client.read_holding_registers(unit_id, address, quantity);
            let regs = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(regs.values()[..regs.quantity() as usize].to_vec())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Reads input registers (FC04).
    #[napi(ts_return_type = "Promise<number[]>")]
    #[cfg(feature = "input-registers")]
    #[doc = "Reads one or more input registers (Function Code 04)."]
    #[doc = "@param opts Options for reading registers."]
    #[doc = "@param opts.address The starting register address."]
    #[doc = "@param opts.quantity The number of registers to read."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_input_registers(
        &self,
        env: Env,
        opts: ReadRegistersOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<u16>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let quantity = opts.quantity;

        let promise = env.spawn_future(async move {
            let fut = client.read_input_registers(unit_id, address, quantity);
            let regs = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(regs.values()[..regs.quantity() as usize].to_vec())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Writes a single register (FC06).
    #[napi(ts_return_type = "Promise<void>")]
    #[cfg(feature = "holding-registers")]
    #[doc = "Writes a single holding register (Function Code 06)."]
    #[doc = "@param opts Options for writing a single register."]
    #[doc = "@param opts.address The register address."]
    #[doc = "@param opts.value The 16-bit value to write."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn write_single_register(
        &self,
        env: Env,
        opts: WriteSingleRegisterOptions<'_>,
    ) -> Result<PromiseRaw<'static, ()>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let value = opts.value;

        let promise = env.spawn_future(async move {
            let fut = client.write_single_register(unit_id, address, value);
            if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Writes multiple registers (FC16).
    #[napi(ts_return_type = "Promise<void>")]
    #[cfg(feature = "holding-registers")]
    #[doc = "Writes multiple consecutive holding registers (Function Code 16)."]
    #[doc = "@param opts Options for writing multiple registers."]
    #[doc = "@param opts.address The starting register address."]
    #[doc = "@param opts.values An array of 16-bit values to write."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn write_multiple_registers(
        &self,
        env: Env,
        opts: WriteMultipleRegistersOptions<'_>,
    ) -> Result<PromiseRaw<'static, ()>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let values = opts.values;

        let promise = env.spawn_future(async move {
            let fut = client.write_multiple_registers(unit_id, address, &values);
            if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Reads and writes multiple registers atomically (FC23).
    #[napi(ts_return_type = "Promise<number[]>")]
    #[cfg(feature = "holding-registers")]
    #[doc = "Performs an atomic read/write operation on multiple registers (Function Code 23)."]
    #[doc = "@param opts Options for the read/write operation."]
    #[doc = "@param opts.read_address The starting address for the read operation."]
    #[doc = "@param opts.read_quantity The number of registers to read."]
    #[doc = "@param opts.write_address The starting address for the write operation."]
    #[doc = "@param opts.write_values An array of 16-bit values to write."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_write_multiple_registers(
        &self,
        env: Env,
        opts: ReadWriteMultipleRegistersOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<u16>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let read_address = opts.read_address;
        let read_quantity = opts.read_quantity;
        let write_address = opts.write_address;
        let write_values = opts.write_values;

        let promise = env.spawn_future(async move {
            let fut = client.read_write_multiple_registers(
                unit_id,
                read_address,
                read_quantity,
                write_address,
                &write_values,
            );
            let regs = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(regs.values()[..regs.quantity() as usize].to_vec())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    // ── Coil methods ─────────────────────────────────────────────────────────

    /// Reads coils (FC01).
    #[napi(ts_return_type = "Promise<boolean[]>")]
    #[cfg(feature = "coils")]
    #[doc = "Reads the status of one or more coils (Function Code 01)."]
    #[doc = "@param opts Options for reading bits."]
    #[doc = "@param opts.address The starting coil address."]
    #[doc = "@param opts.quantity The number of coils to read."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_coils(
        &self,
        env: Env,
        opts: ReadBitsOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<bool>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let quantity = opts.quantity;

        let promise = env.spawn_future(async move {
            let fut = client.read_multiple_coils(unit_id, address, quantity);
            let coils = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };

            let mut result = Vec::with_capacity(quantity as usize);
            for i in 0..quantity {
                result.push(coils.value(address + i).unwrap_or(false));
            }
            Ok(result)
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Writes a single coil (FC05).
    #[napi(ts_return_type = "Promise<void>")]
    #[cfg(feature = "coils")]
    #[doc = "Writes a single coil (Function Code 05)."]
    #[doc = "@param opts Options for writing a single coil."]
    #[doc = "@param opts.address The coil address."]
    #[doc = "@param opts.value The boolean value to write (true for ON, false for OFF)."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn write_single_coil(
        &self,
        env: Env,
        opts: WriteSingleCoilOptions<'_>,
    ) -> Result<PromiseRaw<'static, ()>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let value = opts.value;

        let promise = env.spawn_future(async move {
            let fut = client.write_single_coil(unit_id, address, value);
            if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Writes multiple coils (FC15).
    #[napi(ts_return_type = "Promise<void>")]
    #[cfg(feature = "coils")]
    #[doc = "Writes multiple consecutive coils (Function Code 15)."]
    #[doc = "@param opts Options for writing multiple coils."]
    #[doc = "@param opts.address The starting coil address."]
    #[doc = "@param opts.values An array of boolean values to write."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn write_multiple_coils(
        &self,
        env: Env,
        opts: WriteMultipleCoilsOptions<'_>,
    ) -> Result<PromiseRaw<'static, ()>> {
        use mbus_core::models::coil::Coils;

        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let values = opts.values;

        // Build Coils from bool array synchronously
        let qty = values.len() as u16;
        let mut coils =
            Coils::new(address, qty).map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        for (i, &value) in values.iter().enumerate() {
            coils
                .set_value(address + i as u16, value)
                .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;
        }

        let promise = env.spawn_future(async move {
            let fut = client.write_multiple_coils(unit_id, address, &coils);
            if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    // ── Discrete inputs ──────────────────────────────────────────────────────

    /// Reads discrete inputs (FC02).
    #[napi(ts_return_type = "Promise<boolean[]>")]
    #[cfg(feature = "discrete-inputs")]
    #[doc = "Reads the status of one or more discrete inputs (Function Code 02)."]
    #[doc = "@param opts Options for reading bits."]
    #[doc = "@param opts.address The starting discrete input address."]
    #[doc = "@param opts.quantity The number of discrete inputs to read."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_discrete_inputs(
        &self,
        env: Env,
        opts: ReadBitsOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<bool>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;
        let quantity = opts.quantity;

        let promise = env.spawn_future(async move {
            let fut = client.read_discrete_inputs(unit_id, address, quantity);
            let inputs = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };

            let mut result = Vec::with_capacity(quantity as usize);
            for i in 0..quantity {
                result.push(inputs.value(address + i).unwrap_or(false));
            }
            Ok(result)
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    // ── FIFO ─────────────────────────────────────────────────────────────────

    /// Reads FIFO queue (FC24).
    #[napi(ts_return_type = "Promise<FifoQueueResponse>")]
    #[cfg(feature = "fifo")]
    #[doc = "Reads from a FIFO queue register (Function Code 24)."]
    #[doc = "@param opts Options for reading the FIFO queue."]
    #[doc = "@param opts.address The FIFO pointer address."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_fifo_queue(
        &self,
        env: Env,
        opts: ReadFifoQueueOptions<'_>,
    ) -> Result<PromiseRaw<'static, FifoQueueResponse>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;
        let address = opts.address;

        let promise = env.spawn_future(async move {
            let fut = client.read_fifo_queue(unit_id, address);
            let fifo = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            let values = fifo.queue().to_vec();
            let count = values.len() as u16;
            Ok(FifoQueueResponse { count, values })
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    // ── File record ──────────────────────────────────────────────────────────

    /// Reads file records (FC20).
    #[napi(ts_return_type = "Promise<number[][]>")]
    #[cfg(feature = "file-record")]
    #[doc = "Reads one or more file records (Function Code 20)."]
    #[doc = "@param opts Options for reading file records."]
    #[doc = "@param opts.requests An array of file record read sub-requests."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_file_record(
        &self,
        env: Env,
        opts: ReadFileRecordOptions<'_>,
    ) -> Result<PromiseRaw<'static, Vec<Vec<u16>>>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;

        // Build SubRequest from options
        let mut sub_request = SubRequest::new();
        for req in &opts.requests {
            sub_request
                .add_read_sub_request(req.file_number, req.record_number, req.record_length)
                .map_err(|e| {
                    napi::Error::new(Status::InvalidArg, format!("File record error: {:?}", e))
                })?;
        }

        let promise = env.spawn_future(async move {
            let fut = client.read_file_record(unit_id, &sub_request);
            let result = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };

            // Convert each sub-response to Vec<u16>
            let mut output: Vec<Vec<u16>> = Vec::new();
            for params in result {
                let words: Vec<u16> = params
                    .record_data
                    .map(|v| v.into_iter().collect())
                    .unwrap_or_default();
                output.push(words);
            }
            Ok(output)
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Writes file records (FC21).
    #[napi(ts_return_type = "Promise<void>")]
    #[cfg(feature = "file-record")]
    #[doc = "Writes one or more file records (Function Code 21)."]
    #[doc = "@param opts Options for writing file records."]
    #[doc = "@param opts.requests An array of file record write sub-requests."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn write_file_record(
        &self,
        env: Env,
        opts: WriteFileRecordOptions<'_>,
    ) -> Result<PromiseRaw<'static, ()>> {
        use mbus_core::data_unit::common::MAX_PDU_DATA_LEN;

        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;

        // Build SubRequest with data
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

        let promise = env.spawn_future(async move {
            let fut = client.write_file_record(unit_id, &sub_request);
            if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };
            Ok(())
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    // ── Diagnostics ──────────────────────────────────────────────────────────

    /// Reads exception status (FC07).
    #[napi]
    #[cfg(feature = "diagnostics")]
    #[doc = "Reads the device's exception status byte (Function Code 07)."]
    pub async fn read_exception_status(&self) -> Result<u8> {
        self.inner
            .read_exception_status(self.unit_id)
            .await
            .map_err(from_async_error)
    }

    /// Sends a diagnostics request (FC08).
    #[napi(ts_return_type = "Promise<DiagnosticsResponse>")]
    #[cfg(feature = "diagnostics")]
    #[doc = "Executes a diagnostic function on the device (Function Code 08)."]
    #[doc = "@param opts Options for the diagnostics request."]
    #[doc = "@param opts.sub_function The diagnostic sub-function code."]
    #[doc = "@param opts.data Data to be sent with the request."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn diagnostics(
        &self,
        env: Env,
        opts: DiagnosticsOptions<'_>,
    ) -> Result<PromiseRaw<'static, DiagnosticsResponse>> {
        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;

        let sub_function = DiagnosticSubFunction::try_from(opts.sub_function).map_err(|_| {
            napi::Error::new(
                Status::InvalidArg,
                format!(
                    "Invalid diagnostic sub-function code: {}",
                    opts.sub_function
                ),
            )
        })?;
        let data = opts.data;

        let promise = env.spawn_future(async move {
            let fut = client.diagnostics(unit_id, sub_function, &data);
            let result = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };

            Ok(DiagnosticsResponse {
                sub_function: u16::from(result.sub_function),
                data: result.data,
            })
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Reads device identification (FC43/MEI14).
    #[napi(ts_return_type = "Promise<DeviceIdentificationResponse>")]
    #[cfg(feature = "diagnostics")]
    #[doc = "Reads device identification information (Function Code 43, MEI 14)."]
    #[doc = "@param opts Options for reading device identification."]
    #[doc = "@param opts.read_device_id_code The category of objects to read (e.g., basic, regular)."]
    #[doc = "@param opts.object_id The ID of the first object to read."]
    #[doc = "@param opts.signal An optional `AbortSignal` to cancel the operation."]
    pub fn read_device_identification(
        &self,
        env: Env,
        opts: ReadDeviceIdentificationOptions<'_>,
    ) -> Result<PromiseRaw<'static, DeviceIdentificationResponse>> {
        use mbus_core::models::diagnostic::ConformityLevel;

        let client = self.inner.clone();
        let abort_rx = crate::nodejs::errors::setup_abort_listener(&env, opts.signal)?;
        let unit_id = self.unit_id;

        let read_device_id_code =
            ReadDeviceIdCode::try_from(opts.read_device_id_code).map_err(|_| {
                napi::Error::new(
                    Status::InvalidArg,
                    format!("Invalid read device ID code: {}", opts.read_device_id_code),
                )
            })?;
        let object_id = ObjectId::from(opts.object_id);

        let promise = env.spawn_future(async move {
            let fut = client.read_device_identification(unit_id, read_device_id_code, object_id);
            let result = if let Some(mut rx) = abort_rx {
                tokio::select! {
                    res = fut => { res.map_err(from_async_error)? }
                    _ = &mut rx => {
                        return Err(napi::Error::new(Status::Cancelled, "The operation was aborted."));
                    }
                }
            } else {
                fut.await.map_err(from_async_error)?
            };

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
        })?;
        Ok(unsafe { extend_lifetime(promise) })
    }
}
