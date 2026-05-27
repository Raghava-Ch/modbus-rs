//! Heterogeneous async downstream transport.
//!
//! [`GatewayTransport`] is an enum that wraps any supported async downstream
//! transport — TCP, RTU serial, or ASCII serial — and transparently handles
//! ADU framing translation so the session loop only ever deals with TCP-framed
//! packets internally.

use heapless::Vec;
use mbus_core::data_unit::common::{MAX_ADU_FRAME_LEN, compile_adu_frame, decompile_adu_frame};
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    AsyncTransport, BackoffStrategy, BaudRate, DataBits, JitterStrategy, ModbusConfig,
    ModbusSerialConfig, Parity, SerialMode, TransportType,
};

#[cfg(feature = "downstream-tcp")]
use mbus_network::TokioTcpTransport;

#[cfg(feature = "downstream-serial-rtu")]
use mbus_serial::TokioRtuTransport;

#[cfg(feature = "downstream-serial-ascii")]
use mbus_serial::TokioAsciiTransport;

// ─────────────────────────────────────────────────────────────────────────────
// GatewayTransport — heterogeneous downstream enum
// ─────────────────────────────────────────────────────────────────────────────

/// A heterogeneous async downstream transport.
///
/// Wraps TCP, RTU serial, or ASCII serial transports in a single enum.
/// When the inner transport is serial, ADU framing is automatically translated
/// between Modbus TCP MBAP format (used internally by the session loop) and the
/// wire format of the downstream (RTU CRC or ASCII LRC).
pub enum GatewayTransport {
    /// Modbus TCP downstream.
    #[cfg(feature = "downstream-tcp")]
    Tcp(TokioTcpTransport),
    /// Modbus RTU serial downstream.
    #[cfg(feature = "downstream-serial-rtu")]
    Rtu(TokioRtuTransport),
    /// Modbus ASCII serial downstream.
    #[cfg(feature = "downstream-serial-ascii")]
    Ascii(TokioAsciiTransport),
}

impl AsyncTransport for GatewayTransport {
    /// The nominal transport type exposed to the session loop (always TCP-framed internally).
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;
    const SUPPORTS_BROADCAST_WRITES: bool = true;

    fn transport_type(&self) -> TransportType {
        match self {
            #[cfg(feature = "downstream-tcp")]
            Self::Tcp(_) => TokioTcpTransport::TRANSPORT_TYPE,
            #[cfg(feature = "downstream-serial-rtu")]
            Self::Rtu(_) => TokioRtuTransport::TRANSPORT_TYPE,
            #[cfg(feature = "downstream-serial-ascii")]
            Self::Ascii(_) => TokioAsciiTransport::TRANSPORT_TYPE,
        }
    }

    fn is_connected(&self) -> bool {
        match self {
            #[cfg(feature = "downstream-tcp")]
            Self::Tcp(t) => t.is_connected(),
            #[cfg(feature = "downstream-serial-rtu")]
            Self::Rtu(_) => true,
            #[cfg(feature = "downstream-serial-ascii")]
            Self::Ascii(_) => true,
        }
    }

    async fn send<'a>(&'a mut self, adu: &'a [u8]) -> Result<(), MbusError> {
        match self {
            #[cfg(feature = "downstream-tcp")]
            Self::Tcp(t) => t.send(adu).await,
            #[cfg(feature = "downstream-serial-rtu")]
            Self::Rtu(t) => {
                // Translate from TCP MBAP → RTU CRC
                let msg = decompile_adu_frame(adu, TransportType::StdTcp)?;
                let unit = msg.unit_id_or_slave_addr().get();
                let wire = compile_adu_frame(0, unit, msg.pdu, TokioRtuTransport::TRANSPORT_TYPE)?;
                t.send(&wire).await
            }
            #[cfg(feature = "downstream-serial-ascii")]
            Self::Ascii(t) => {
                // Translate from TCP MBAP → ASCII LRC
                let msg = decompile_adu_frame(adu, TransportType::StdTcp)?;
                let unit = msg.unit_id_or_slave_addr().get();
                let wire =
                    compile_adu_frame(0, unit, msg.pdu, TokioAsciiTransport::TRANSPORT_TYPE)?;
                t.send(&wire).await
            }
        }
    }

    async fn recv(&mut self) -> Result<Vec<u8, { MAX_ADU_FRAME_LEN }>, MbusError> {
        match self {
            #[cfg(feature = "downstream-tcp")]
            Self::Tcp(t) => t.recv().await,
            #[cfg(feature = "downstream-serial-rtu")]
            Self::Rtu(t) => {
                // Translate RTU CRC → TCP MBAP
                let wire = t.recv().await?;
                let msg = decompile_adu_frame(&wire, TokioRtuTransport::TRANSPORT_TYPE)?;
                let unit = msg.unit_id_or_slave_addr().get();
                compile_adu_frame(0, unit, msg.pdu, TransportType::StdTcp)
            }
            #[cfg(feature = "downstream-serial-ascii")]
            Self::Ascii(t) => {
                // Translate ASCII LRC → TCP MBAP
                let wire = t.recv().await?;
                let msg = decompile_adu_frame(&wire, TokioAsciiTransport::TRANSPORT_TYPE)?;
                let unit = msg.unit_id_or_slave_addr().get();
                compile_adu_frame(0, unit, msg.pdu, TransportType::StdTcp)
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// DownstreamConfig — config-driven builder
// ─────────────────────────────────────────────────────────────────────────────

// Transport config type re-exports (so callers only need mbus_gateway::downstream::*)
#[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
pub use mbus_core::transport::Parity as SerialParity;

/// Configuration for a single serial downstream channel.
#[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
#[derive(Debug, Clone)]
pub struct SerialDownstreamConfig {
    pub port: String,
    pub mode: SerialMode,
    pub baud_rate: BaudRate,
    pub data_bits: DataBits,
    pub stop_bits: u8,
    pub parity: Parity,
    pub response_timeout_ms: u32,
    pub retry_attempts: u8,
}

/// Configuration for a downstream channel.
///
/// Build one from your TOML schema fields, then call [`.connect()`](DownstreamConfig::connect)
/// to get a ready-to-use [`GatewayTransport`].
#[derive(Debug, Clone)]
pub enum DownstreamConfig {
    /// TCP downstream — connects outbound to a Modbus TCP slave.
    #[cfg(feature = "downstream-tcp")]
    Tcp { address: String },
    /// Serial downstream — opens an RS-485/RS-232 port.
    #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
    Serial(SerialDownstreamConfig),
}

impl DownstreamConfig {
    /// Connect and return a [`GatewayTransport`] ready to use.
    pub async fn connect(self) -> Result<GatewayTransport, DownstreamConnectError> {
        match self {
            #[cfg(feature = "downstream-tcp")]
            DownstreamConfig::Tcp { address } => {
                let t = TokioTcpTransport::connect(&address)
                    .await
                    .map_err(|e| DownstreamConnectError::Tcp(address.clone(), e))?;
                Ok(GatewayTransport::Tcp(t))
            }
            #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
            DownstreamConfig::Serial(sc) => {
                use std::str::FromStr;
                let port_path = heapless::String::<64>::from_str(&sc.port)
                    .map_err(|_| DownstreamConnectError::PortPathTooLong(sc.port.clone()))?;

                let modbus_cfg = ModbusConfig::Serial(ModbusSerialConfig {
                    port_path,
                    mode: sc.mode,
                    baud_rate: sc.baud_rate,
                    data_bits: sc.data_bits,
                    stop_bits: sc.stop_bits,
                    parity: sc.parity,
                    response_timeout_ms: sc.response_timeout_ms,
                    retry_attempts: sc.retry_attempts,
                    retry_backoff_strategy: BackoffStrategy::Immediate,
                    retry_jitter_strategy: JitterStrategy::None,
                    retry_random_fn: None,
                });

                match sc.mode {
                    #[cfg(feature = "downstream-serial-rtu")]
                    SerialMode::Rtu => {
                        let t = TokioRtuTransport::new(&modbus_cfg)
                            .map_err(|e| DownstreamConnectError::Serial(sc.port.clone(), e))?;
                        Ok(GatewayTransport::Rtu(t))
                    }
                    #[cfg(feature = "downstream-serial-ascii")]
                    SerialMode::Ascii => {
                        let t = TokioAsciiTransport::new(&modbus_cfg)
                            .map_err(|e| DownstreamConnectError::Serial(sc.port.clone(), e))?;
                        Ok(GatewayTransport::Ascii(t))
                    }
                    #[allow(unreachable_patterns)]
                    _ => Err(DownstreamConnectError::UnsupportedMode(sc.mode)),
                }
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Error type
// ─────────────────────────────────────────────────────────────────────────────

/// Errors that can occur when connecting a downstream channel.
#[derive(Debug)]
pub enum DownstreamConnectError {
    #[cfg(feature = "downstream-tcp")]
    Tcp(String, MbusError),
    #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
    Serial(String, MbusError),
    #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
    PortPathTooLong(String),
    #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
    UnsupportedMode(SerialMode),
}

impl core::fmt::Display for DownstreamConnectError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            #[cfg(feature = "downstream-tcp")]
            Self::Tcp(addr, e) => write!(f, "TCP downstream '{addr}' connect failed: {e:?}"),
            #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
            Self::Serial(port, e) => write!(f, "serial downstream '{port}' open failed: {e:?}"),
            #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
            Self::PortPathTooLong(p) => write!(f, "serial port path too long: '{p}'"),
            #[cfg(any(feature = "downstream-serial-rtu", feature = "downstream-serial-ascii"))]
            Self::UnsupportedMode(m) => write!(f, "unsupported serial mode: {m:?}"),
        }
    }
}

impl std::error::Error for DownstreamConnectError {}
