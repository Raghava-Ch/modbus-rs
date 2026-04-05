use core::ffi::{CStr, c_char};

use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    ModbusTcpConfig, Parity, SerialMode,
};

use super::error::MbusStatusCode;

/// Backoff strategy selector for retry logic.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbusBackoffStrategy {
    /// Retry immediately with no delay.
    MbusBackoffImmediate = 0,
    /// Retry after a fixed delay (`backoff_base_delay_ms`).
    MbusBackoffFixed,
    /// Retry with exponentially increasing delay, capped at `backoff_max_delay_ms`.
    MbusBackoffExponential,
    /// Retry with linearly increasing delay, capped at `backoff_max_delay_ms`.
    MbusBackoffLinear,
}

/// Serial framing mode.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbusSerialMode {
    /// Modbus RTU binary framing with CRC-16.
    MbusSerialRtu = 0,
    /// Modbus ASCII framing with LRC.
    MbusSerialAscii,
}

/// Configuration for a Modbus TCP client.
///
/// All pointer fields (`host`) must remain valid for the duration of the
/// `mbus_tcp_client_new` call. They are copied internally and do not need to
/// outlive the call.
#[repr(C)]
pub struct MbusTcpConfig {
    /// Null-terminated hostname or IPv4/IPv6 address string (max 63 bytes excl. NUL).
    pub host: *const c_char,
    /// TCP port (default Modbus port is 502).
    pub port: u16,
    /// Timeout waiting for the TCP connection to be established, in milliseconds.
    pub connection_timeout_ms: u32,
    /// Timeout waiting for a Modbus response, in milliseconds.
    pub response_timeout_ms: u32,
    /// Number of retry attempts before reporting failure via the error callback.
    pub retries: u8,
    /// Backoff strategy between retries.
    pub backoff_strategy: MbusBackoffStrategy,
    /// Base delay (ms) used by `MbusBackoffFixed`, `MbusBackoffExponential`, and
    /// `MbusBackoffLinear`.
    pub backoff_base_delay_ms: u32,
    /// Maximum delay cap (ms) used by `MbusBackoffExponential` and `MbusBackoffLinear`.
    pub backoff_max_delay_ms: u32,
    /// Jitter percentage (0 = no jitter, 1–100 = ±N% random spread on top of backoff).
    pub jitter_percent: u8,
}

/// Configuration for a Modbus Serial (RTU or ASCII) client.
///
/// `port_name` must remain valid for the duration of `mbus_serial_client_new`.
#[repr(C)]
pub struct MbusSerialConfig {
    /// Null-terminated serial port path (e.g. `"/dev/ttyUSB0"` or `"COM3"`).
    pub port_name: *const c_char,
    /// Baud rate (e.g. 9600, 19200, 115200).
    pub baud_rate: u32,
    /// Framing mode: RTU or ASCII.
    pub mode: MbusSerialMode,
    /// Timeout waiting for a Modbus response, in milliseconds.
    pub response_timeout_ms: u32,
    /// Number of retry attempts.
    pub retries: u8,
    /// Backoff strategy between retries.
    pub backoff_strategy: MbusBackoffStrategy,
    /// Base delay (ms).
    pub backoff_base_delay_ms: u32,
    /// Maximum delay cap (ms).
    pub backoff_max_delay_ms: u32,
    /// Jitter percentage (0–100).
    pub jitter_percent: u8,
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn map_backoff(strategy: MbusBackoffStrategy, base: u32, max: u32) -> BackoffStrategy {
    match strategy {
        MbusBackoffStrategy::MbusBackoffImmediate => BackoffStrategy::Immediate,
        MbusBackoffStrategy::MbusBackoffFixed => BackoffStrategy::Fixed { delay_ms: base },
        MbusBackoffStrategy::MbusBackoffExponential => {
            BackoffStrategy::Exponential { base_delay_ms: base, max_delay_ms: max }
        }
        MbusBackoffStrategy::MbusBackoffLinear => BackoffStrategy::Linear {
            initial_delay_ms: base,
            increment_ms: base,
            max_delay_ms: max,
        },
    }
}

fn map_jitter(percent: u8) -> JitterStrategy {
    if percent == 0 {
        JitterStrategy::None
    } else {
        JitterStrategy::Percentage { percent }
    }
}

/// Convert a `*const MbusTcpConfig` into an owned `ModbusConfig::Tcp`.
///
/// # Safety
/// `cfg` must be a valid non-null pointer to an initialised `MbusTcpConfig`.
pub(super) unsafe fn tcp_config_from_c(
    cfg: *const MbusTcpConfig,
) -> Result<ModbusConfig, MbusStatusCode> {
    if cfg.is_null() {
        return Err(MbusStatusCode::MbusErrNullPointer);
    }
    let cfg = unsafe { &*cfg };

    if cfg.host.is_null() {
        return Err(MbusStatusCode::MbusErrNullPointer);
    }
    let host_str = unsafe { CStr::from_ptr(cfg.host) }
        .to_str()
        .map_err(|_| MbusStatusCode::MbusErrInvalidUtf8)?;

    let inner = ModbusTcpConfig::new(host_str, cfg.port)
        .map_err(MbusStatusCode::from)?;

    Ok(ModbusConfig::Tcp(ModbusTcpConfig {
        connection_timeout_ms: cfg.connection_timeout_ms,
        response_timeout_ms: cfg.response_timeout_ms,
        retry_attempts: cfg.retries,
        retry_backoff_strategy: map_backoff(
            cfg.backoff_strategy,
            cfg.backoff_base_delay_ms,
            cfg.backoff_max_delay_ms,
        ),
        retry_jitter_strategy: map_jitter(cfg.jitter_percent),
        retry_random_fn: None,
        ..inner
    }))
}

/// Convert a `*const MbusSerialConfig` into an owned `ModbusConfig::Serial`.
///
/// # Safety
/// `cfg` must be a valid non-null pointer to an initialised `MbusSerialConfig`.
pub(super) unsafe fn serial_config_from_c(
    cfg: *const MbusSerialConfig,
) -> Result<ModbusSerialConfig, MbusStatusCode> {
    if cfg.is_null() {
        return Err(MbusStatusCode::MbusErrNullPointer);
    }
    let cfg = unsafe { &*cfg };

    if cfg.port_name.is_null() {
        return Err(MbusStatusCode::MbusErrNullPointer);
    }
    let port_str = unsafe { CStr::from_ptr(cfg.port_name) }
        .to_str()
        .map_err(|_| MbusStatusCode::MbusErrInvalidUtf8)?;

    let mode = match cfg.mode {
        MbusSerialMode::MbusSerialRtu => SerialMode::Rtu,
        MbusSerialMode::MbusSerialAscii => SerialMode::Ascii,
    };

    let port_path = heapless::String::<64>::try_from(port_str)
        .map_err(|_| MbusStatusCode::MbusErrBufferTooSmall)?;

    Ok(ModbusSerialConfig {
        port_path,
        mode,
        baud_rate: BaudRate::Custom(cfg.baud_rate),
        data_bits: DataBits::Eight,
        parity: Parity::None,
        stop_bits: 1,
        response_timeout_ms: cfg.response_timeout_ms,
        retry_attempts: cfg.retries,
        retry_backoff_strategy: map_backoff(
            cfg.backoff_strategy,
            cfg.backoff_base_delay_ms,
            cfg.backoff_max_delay_ms,
        ),
        retry_jitter_strategy: map_jitter(cfg.jitter_percent),
        retry_random_fn: None,
    })
}
