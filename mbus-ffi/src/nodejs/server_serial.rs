//! Node.js bindings for the async Modbus serial server.

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

// ── Option structs ───────────────────────────────────────────────────────────

/// Server bind options for serial port.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SerialServerOptions {
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
}

// ── Helpers ──────────────────────────────────────────────────────────────────

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

fn parse_baud_rate(rate: u32) -> Result<BaudRate> {
    match rate {
        9600 => Ok(BaudRate::Baud9600),
        19200 => Ok(BaudRate::Baud19200),
        _ => Ok(BaudRate::Custom(rate)),
    }
}

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

fn parse_stop_bits(bits: u8) -> Result<u8> {
    match bits {
        1 | 2 => Ok(bits),
        _ => Err(napi::Error::new(
            Status::InvalidArg,
            format!("Invalid stop bits: {}. Expected 1 or 2", bits),
        )),
    }
}

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
pub struct AsyncSerialModbusServer {
    stop_signal: Arc<Notify>,
    join_handle: Mutex<Option<JoinHandle<()>>>,
}

#[napi]
impl AsyncSerialModbusServer {
    /// Binds and starts a new Serial RTU server.
    #[napi(factory)]
    pub fn bind_rtu(
        env: Env,
        opts: SerialServerOptions,
        handlers: Object,
    ) -> Result<AsyncSerialModbusServer> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let serial_config = build_serial_config(&opts, SerialMode::Rtu)?;
        let config = mbus_core::transport::ModbusConfig::Serial(serial_config);

        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the handler adapter
        let adapter = crate::nodejs::server_tcp::build_adapter(&env, &handlers)?;

        // Spawn the server task
        let rt = runtime::get();
        let join_handle = rt.spawn(async move {
            let mut server = match mbus_server_async::AsyncRtuServer::new_rtu(&config, unit) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create Serial RTU Server: {:?}", e);
                    return;
                }
            };
            let _ = server
                .run_with_shutdown(adapter, stop_signal_clone.notified())
                .await;
        });

        Ok(AsyncSerialModbusServer {
            stop_signal,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    /// Binds and starts a new Serial ASCII server.
    #[napi(factory)]
    pub fn bind_ascii(
        env: Env,
        opts: SerialServerOptions,
        handlers: Object,
    ) -> Result<AsyncSerialModbusServer> {
        let unit = UnitIdOrSlaveAddr::new(opts.unit_id)
            .map_err(|e| to_napi_err(ERR_MODBUS_INVALID_ARGUMENT, e))?;

        let serial_config = build_serial_config(&opts, SerialMode::Ascii)?;
        let config = mbus_core::transport::ModbusConfig::Serial(serial_config);

        let stop_signal = Arc::new(Notify::new());
        let stop_signal_clone = stop_signal.clone();

        // Build the handler adapter
        let adapter = crate::nodejs::server_tcp::build_adapter(&env, &handlers)?;

        // Spawn the server task
        let rt = runtime::get();
        let join_handle = rt.spawn(async move {
            let mut server = match mbus_server_async::AsyncAsciiServer::new_ascii(&config, unit) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("Failed to create Serial ASCII Server: {:?}", e);
                    return;
                }
            };
            let _ = server
                .run_with_shutdown(adapter, stop_signal_clone.notified())
                .await;
        });

        Ok(AsyncSerialModbusServer {
            stop_signal,
            join_handle: Mutex::new(Some(join_handle)),
        })
    }

    /// Stops the server.
    #[napi]
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
