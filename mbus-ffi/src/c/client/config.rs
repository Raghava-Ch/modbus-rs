use core::ffi::{CStr, c_char};

use mbus_core::transport::{
    BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig, ModbusSerialConfig,
    ModbusTcpConfig, Parity, SerialMode,
};

use crate::c::error::MbusStatusCode;

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
        MbusBackoffStrategy::MbusBackoffExponential => BackoffStrategy::Exponential {
            base_delay_ms: base,
            max_delay_ms: max,
        },
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

    let inner = ModbusTcpConfig::new(host_str, cfg.port).map_err(MbusStatusCode::from)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::c_char;

    // ── tcp_config_from_c ─────────────────────────────────────────────────────

    #[test]
    fn tcp_null_cfg_returns_null_pointer_error() {
        let result = unsafe { tcp_config_from_c(core::ptr::null()) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrNullPointer)));
    }

    #[test]
    fn tcp_null_host_returns_null_pointer_error() {
        let cfg = MbusTcpConfig {
            host: core::ptr::null(),
            port: 502,
            connection_timeout_ms: 1000,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { tcp_config_from_c(&cfg) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrNullPointer)));
    }

    #[test]
    fn tcp_host_too_long_returns_error() {
        // 65 chars + NUL = 66 bytes total; heapless::String:<64> capacity is 64.
        let long: [u8; 66] = {
            let mut a = [b'a'; 66];
            a[65] = 0;
            a
        };
        let cfg = MbusTcpConfig {
            host: long.as_ptr() as *const c_char,
            port: 502,
            connection_timeout_ms: 1000,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { tcp_config_from_c(&cfg) };
        assert!(
            result.is_err(),
            "expected error for host > 63 bytes, got Ok"
        );
    }

    #[test]
    fn tcp_invalid_utf8_host_returns_error() {
        // 0xFF is not valid UTF-8.
        let bad: [u8; 2] = [0xFF, 0x00];
        let cfg = MbusTcpConfig {
            host: bad.as_ptr() as *const c_char,
            port: 502,
            connection_timeout_ms: 1000,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { tcp_config_from_c(&cfg) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrInvalidUtf8)));
    }

    #[test]
    fn tcp_valid_config_round_trips() {
        let host = b"192.168.1.1\0";
        let cfg = MbusTcpConfig {
            host: host.as_ptr() as *const c_char,
            port: 502,
            connection_timeout_ms: 2000,
            response_timeout_ms: 3000,
            retries: 5,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffFixed,
            backoff_base_delay_ms: 100,
            backoff_max_delay_ms: 0,
            jitter_percent: 10,
        };
        let result = unsafe { tcp_config_from_c(&cfg) };
        assert!(
            result.is_ok(),
            "expected Ok for valid config, got {:?}",
            result
        );
        if let Ok(ModbusConfig::Tcp(inner)) = result {
            assert_eq!(inner.port, 502);
            assert_eq!(inner.response_timeout_ms, 3000);
            assert_eq!(inner.retry_attempts, 5);
        } else {
            panic!("expected ModbusConfig::Tcp");
        }
    }

    // ── serial_config_from_c ──────────────────────────────────────────────────

    #[test]
    fn serial_null_cfg_returns_null_pointer_error() {
        let result = unsafe { serial_config_from_c(core::ptr::null()) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrNullPointer)));
    }

    #[test]
    fn serial_null_port_name_returns_null_pointer_error() {
        let cfg = MbusSerialConfig {
            port_name: core::ptr::null(),
            baud_rate: 9600,
            mode: MbusSerialMode::MbusSerialRtu,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { serial_config_from_c(&cfg) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrNullPointer)));
    }

    #[test]
    fn serial_port_name_too_long_returns_error() {
        // 64 bytes of 'x' + NUL; heapless::String::<64> can only hold 63 chars + NUL.
        let long: [u8; 66] = {
            let mut a = [b'x'; 66];
            a[65] = 0;
            a
        };
        let cfg = MbusSerialConfig {
            port_name: long.as_ptr() as *const c_char,
            baud_rate: 9600,
            mode: MbusSerialMode::MbusSerialRtu,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { serial_config_from_c(&cfg) };
        assert!(result.is_err(), "expected error for port_name > 63 bytes");
    }

    #[test]
    fn serial_invalid_utf8_port_name_returns_error() {
        let bad: [u8; 2] = [0xFF, 0x00];
        let cfg = MbusSerialConfig {
            port_name: bad.as_ptr() as *const c_char,
            baud_rate: 9600,
            mode: MbusSerialMode::MbusSerialRtu,
            response_timeout_ms: 1000,
            retries: 3,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { serial_config_from_c(&cfg) };
        assert!(matches!(result, Err(MbusStatusCode::MbusErrInvalidUtf8)));
    }

    #[test]
    fn serial_valid_config_round_trips() {
        let port = b"/dev/ttyUSB0\0";
        let cfg = MbusSerialConfig {
            port_name: port.as_ptr() as *const c_char,
            baud_rate: 115200,
            mode: MbusSerialMode::MbusSerialAscii,
            response_timeout_ms: 500,
            retries: 2,
            backoff_strategy: MbusBackoffStrategy::MbusBackoffImmediate,
            backoff_base_delay_ms: 0,
            backoff_max_delay_ms: 0,
            jitter_percent: 0,
        };
        let result = unsafe { serial_config_from_c(&cfg) };
        assert!(
            result.is_ok(),
            "expected Ok for valid config, got {:?}",
            result
        );
        let inner = result.unwrap();
        assert_eq!(inner.response_timeout_ms, 500);
        assert_eq!(inner.retry_attempts, 2);
    }

    #[test]
    fn map_backoff_immediate_variant() {
        let s = map_backoff(MbusBackoffStrategy::MbusBackoffImmediate, 0, 0);
        assert!(matches!(s, BackoffStrategy::Immediate));
    }

    #[test]
    fn map_backoff_fixed_variant() {
        let s = map_backoff(MbusBackoffStrategy::MbusBackoffFixed, 50, 0);
        assert!(matches!(s, BackoffStrategy::Fixed { delay_ms: 50 }));
    }

    #[test]
    fn map_jitter_zero_is_none() {
        assert!(matches!(map_jitter(0), JitterStrategy::None));
    }

    #[test]
    fn map_jitter_nonzero_is_percentage() {
        assert!(matches!(
            map_jitter(25),
            JitterStrategy::Percentage { percent: 25 }
        ));
    }
}
