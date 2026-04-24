//! Server-side configuration passed to `mbus_tcp_server_new` / `mbus_serial_server_new`.
//!
//! [`MbusServerConfig`] is a C-friendly configuration struct. Internally it is
//! converted to the `mbus-server` types (`ModbusConfig`, `UnitIdOrSlaveAddr`,
//! `ResilienceConfig`) before being handed to `ServerServices`.

use mbus_core::{
    errors::MbusError,
    transport::{
        BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig,
        ModbusSerialConfig, ModbusTcpConfig, Parity, SerialMode, UnitIdOrSlaveAddr,
    },
};
use mbus_server::ResilienceConfig;

// ── MbusServerConfig ──────────────────────────────────────────────────────────

/// Common server configuration shared by TCP and Serial server types.
///
/// Only fields that influence the server runtime logic are included here;
/// transport-level details (host address, port, baud rate) are handled
/// separately in the `_new` constructor callbacks.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MbusServerConfig {
    /// Slave/unit address this server will respond to.  Valid range: 1–247.
    /// For TCP-only deployments the conventional value is 1 or 0xFF.
    pub slave_address: u8,

    /// Maximum time (ms) allocated to process and respond to a single request.
    /// Passed to `ResilienceConfig::default()` for now; reserved for future
    /// per-request timeout enforcement.
    pub response_timeout_ms: u32,
}

impl MbusServerConfig {
    /// Converts to a `UnitIdOrSlaveAddr`, validating the slave address.
    pub(super) fn unit_id_or_slave_addr(&self) -> Result<UnitIdOrSlaveAddr, MbusError> {
        UnitIdOrSlaveAddr::new(self.slave_address)
    }

    /// Builds a `ResilienceConfig` from this config.
    pub(super) fn resilience(&self) -> ResilienceConfig {
        ResilienceConfig::default()
    }

    /// Builds a `ModbusConfig` suitable for a TCP server (MBAP framing).
    ///
    /// The host/port fields in `ModbusTcpConfig` are not used by the server
    /// runtime (transport is handled via callbacks). Dummy values are used.
    pub(super) fn tcp_modbus_config(&self) -> Result<ModbusConfig, MbusError> {
        let inner = ModbusTcpConfig::new("", 502).map_err(|_| MbusError::InvalidConfiguration)?;
        Ok(ModbusConfig::Tcp(inner))
    }

    /// Builds a `ModbusConfig` suitable for an RTU serial server.
    pub(super) fn rtu_modbus_config(&self) -> Result<ModbusConfig, MbusError> {
        let inner = ModbusSerialConfig {
            port_path: heapless::String::new(),
            mode: SerialMode::Rtu,
            baud_rate: BaudRate::default(),
            data_bits: DataBits::default(),
            stop_bits: 1,
            parity: Parity::default(),
            response_timeout_ms: self.response_timeout_ms,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        };
        Ok(ModbusConfig::Serial(inner))
    }

    /// Builds a `ModbusConfig` suitable for an ASCII serial server.
    pub(super) fn ascii_modbus_config(&self) -> Result<ModbusConfig, MbusError> {
        let inner = ModbusSerialConfig {
            port_path: heapless::String::new(),
            mode: SerialMode::Ascii,
            baud_rate: BaudRate::default(),
            data_bits: DataBits::default(),
            stop_bits: 1,
            parity: Parity::default(),
            response_timeout_ms: self.response_timeout_ms,
            retry_attempts: 0,
            retry_backoff_strategy: BackoffStrategy::Immediate,
            retry_jitter_strategy: JitterStrategy::None,
            retry_random_fn: None,
        };
        Ok(ModbusConfig::Serial(inner))
    }
}

impl Default for MbusServerConfig {
    fn default() -> Self {
        Self {
            slave_address: 1,
            response_timeout_ms: 5_000,
        }
    }
}
