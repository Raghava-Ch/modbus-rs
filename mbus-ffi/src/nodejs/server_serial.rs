//! Node.js bindings for the async Modbus serial server.

use mbus_server_async::{AsyncAsciiServer, AsyncRtuServer};
use std::sync::{Arc, Mutex};

use napi::bindgen_prelude::*;
use napi_derive::napi;

use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusSerialConfig, Parity, SerialMode,
    UnitIdOrSlaveAddr,
};
use tokio::sync::Notify;
use tokio::task::JoinHandle;

use crate::nodejs::errors::{ERR_MODBUS_INVALID_ARGUMENT, to_napi_err};
use crate::nodejs::runtime;

unsafe fn extend_lifetime<'a, 'b, T>(p: PromiseRaw<'a, T>) -> PromiseRaw<'b, T> {
    unsafe { std::mem::transmute(p) }
}

// ── Option structs ───────────────────────────────────────────────────────────

/// Server bind options for serial port.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SerialServerOptions {
    #[doc = "Serial port path (e.g., \"/dev/ttyUSB0\", \"COM3\")."]
    pub port_path: String,
    #[doc = "Baud rate (e.g., 9600, 19200, 38400, 57600, 115200)."]
    pub baud_rate: u32,
    #[doc = "Data bits (5, 6, 7, or 8)."]
    pub data_bits: Option<u8>,
    #[doc = "Parity (\"none\", \"even\", \"odd\")."]
    pub parity: Option<String>,
    #[doc = "Stop bits (1 or 2)."]
    pub stop_bits: Option<u8>,
    #[doc = "Modbus unit ID (1-247) for the server to respond to."]
    pub unit_id: u8,
    #[doc = "Response timeout in milliseconds."]
    pub response_timeout_ms: Option<u32>,
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Parses a string parity value to the `Parity` enum.
fn parse_parity(s: &str) -> Result<Parity> {
    match s.to_lowercase().as_str() {
        "none" | "n" => Ok(Parity::None),
        "even" | "e" => Ok(Parity::Even),
        "odd" | "o" => Ok(Parity::Odd),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!(
                "Invalid parity value: '{}'. Expected 'none', 'even', or 'odd'",
                s
            ),
        )),
    }
}

/// Converts a numeric baud rate to the `BaudRate` enum.
fn parse_baud_rate(rate: u32) -> Result<BaudRate> {
    match rate {
        9600 => Ok(BaudRate::Baud9600),
        19200 => Ok(BaudRate::Baud19200),
        _ => Ok(BaudRate::Custom(rate)),
    }
}

/// Converts numeric data bits to the `DataBits` enum.
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

/// Converts numeric stop bits to a validated `u8`.
fn parse_stop_bits(bits: u8) -> Result<u8> {
    match bits {
        1 | 2 => Ok(bits),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!("Invalid stop bits: {}. Expected 1 or 2", bits),
        )),
    }
}

/// Builds a `ModbusSerialConfig` from the provided options.
fn build_serial_config(opts: &SerialServerOptions, mode: SerialMode) -> Result<ModbusSerialConfig> {
    let baud_rate = parse_baud_rate(opts.baud_rate)?;
    let data_bits = opts
        .data_bits
        .map(parse_data_bits)
        .transpose()?
        .unwrap_or(DataBits::Eight);
    let parity = opts
        .parity
        .as_ref()
        .map(|s| parse_parity(s))
        .transpose()?
        .unwrap_or(Parity::None);
    let stop_bits = opts
        .stop_bits
        .map(parse_stop_bits)
        .transpose()?
        .unwrap_or(1);
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

// ── AsyncSerialModbusServer ──────────────────────────────────────────────────

/// Async Modbus Serial server supporting RTU and ASCII transports.
#[napi]
#[doc = "An asynchronous Modbus server that listens on a serial port for incoming requests."]
#[doc = "It supports both RTU and ASCII transport modes."]
#[doc = "Use the static `bindRtu` or `bindAscii` methods to create and start a server instance."]
pub struct AsyncSerialModbusServer {
    stop_signal: Arc<Notify>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

#[napi]
impl AsyncSerialModbusServer {
    #[doc = "Creates and starts a new Modbus RTU server on a serial port."]
    #[doc = ""]
    #[doc = "@param opts Server configuration options."]
    #[doc = "@param opts.portPath The path to the serial port (e.g., '/dev/ttyUSB0', 'COM3')."]
    #[doc = "@param opts.baudRate The communication speed (e.g., 9600, 19200)."]
    #[doc = "@param opts.unitId The Modbus unit ID the server will respond to."]
    #[doc = "@param opts.dataBits Optional number of data bits (5, 6, 7, or 8). Defaults to 8."]
    #[doc = "@param opts.parity Optional parity setting ('none', 'even', 'odd'). Defaults to 'none'."]
    #[doc = "@param opts.stopBits Optional number of stop bits (1 or 2). Defaults to 1."]
    #[doc = "@param opts.responseTimeoutMs Optional response timeout in milliseconds. Defaults to 1000."]
    #[doc = ""]
    #[doc = "@param handlers An object containing callback functions to handle Modbus requests (matches the `ServerHandlers` interface in TypeScript)."]
    #[doc = "@returns A `Promise` that resolves to a running `AsyncSerialModbusServer` instance."]
    #[allow(clippy::missing_transmute_annotations)]
    pub fn bind_rtu(
        env: Env,
        opts: SerialServerOptions,
        handlers: Object<'_>,
    ) -> Result<PromiseRaw<'static, AsyncSerialModbusServer>> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let serial_config = build_serial_config(&opts, SerialMode::Rtu)?;
        let config = mbus_core::transport::ModbusConfig::Serial(serial_config);

        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the handler adapter
        let adapter = crate::nodejs::server_tcp::build_adapter(&env, &handlers)?;

        let promise = env.spawn_future(async move {
            // Try creating/binding the server first so we catch serial port errors
            let mut server = AsyncRtuServer::new_rtu(&config, unit).map_err(|e| {
                napi::Error::new(Status::GenericFailure, format!("Bind RTU failed: {:?}", e))
            })?;

            // Spawn the server task
            let rt = runtime::get();
            let join_handle = rt.spawn(async move {
                let _ = server
                    .run_with_shutdown(adapter, stop_signal_clone.notified())
                    .await;
            });

            Ok(AsyncSerialModbusServer {
                stop_signal,
                join_handle: Mutex::new(Some(join_handle)),
            })
        })?;

        Ok(unsafe { extend_lifetime(promise) })
    }

    #[doc = "Creates and starts a new Modbus ASCII server on a serial port."]
    #[doc = ""]
    #[doc = "@param opts Server configuration options."]
    #[doc = "@param opts.portPath The path to the serial port (e.g., '/dev/ttyUSB0', 'COM3')."]
    #[doc = "@param opts.baudRate The communication speed (e.g., 9600, 19200)."]
    #[doc = "@param opts.unitId The Modbus unit ID the server will respond to."]
    #[doc = "@param opts.dataBits Optional number of data bits (7 or 8). Defaults to 8."]
    #[doc = "@param opts.parity Optional parity setting ('none', 'even', 'odd'). Defaults to 'none'."]
    #[doc = "@param opts.stopBits Optional number of stop bits (1 or 2). Defaults to 1."]
    #[doc = "@param opts.responseTimeoutMs Optional response timeout in milliseconds. Defaults to 1000."]
    #[doc = ""]
    #[doc = "@param handlers An object containing callback functions to handle Modbus requests (matches the `ServerHandlers` interface in TypeScript)."]
    #[doc = "@returns A `Promise` that resolves to a running `AsyncSerialModbusServer` instance."]
    #[allow(clippy::missing_transmute_annotations)]
    pub fn bind_ascii(
        env: Env,
        opts: SerialServerOptions,
        handlers: Object<'_>,
    ) -> Result<PromiseRaw<'static, AsyncSerialModbusServer>> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let serial_config = build_serial_config(&opts, SerialMode::Ascii)?;
        let config = mbus_core::transport::ModbusConfig::Serial(serial_config);

        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the handler adapter
        let adapter = crate::nodejs::server_tcp::build_adapter(&env, &handlers)?;

        let promise = env.spawn_future(async move {
            // Try creating/binding the server first so we catch serial port errors
            let mut server = AsyncAsciiServer::new_ascii(&config, unit).map_err(|e| {
                napi::Error::new(
                    Status::GenericFailure,
                    format!("Bind ASCII failed: {:?}", e),
                )
            })?;

            // Spawn the server task
            let rt = runtime::get();
            let join_handle = rt.spawn(async move {
                let _ = server
                    .run_with_shutdown(adapter, stop_signal_clone.notified())
                    .await;
            });

            Ok(AsyncSerialModbusServer {
                stop_signal,
                join_handle: Mutex::new(Some(join_handle)),
            })
        })?;

        Ok(unsafe { extend_lifetime(promise) })
    }

    /// Stops the server.
    #[napi]
    #[doc = "Stops the server and closes the serial port."]
    pub async fn shutdown(&self) -> Result<()> {
        self.stop_signal.notify_one();

        let handle = {
            let mut guard = self
                .join_handle
                .lock()
                .map_err(|_| napi::Error::new(Status::GenericFailure, "Failed to acquire lock"))?;
            guard.take()
        };
        if let Some(h) = handle {
            let _ = h.await;
        }

        Ok(())
    }
}
