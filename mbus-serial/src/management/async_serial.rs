use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{
    AsyncTransport, BaudRate, DataBits, ModbusConfig, Parity, SerialMode, TransportType,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{Duration, timeout};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

/// Minimum inter-frame silence imposed by the Modbus spec regardless of baud rate.
///
/// The spec mandates ≥ 1.75 ms at baud rates ≤ 19 200 bps; at higher rates the
/// computed 3.5-character time shrinks below this floor, so we clamp it.
const MIN_INTER_FRAME_US: u64 = 1750;

/// Tokio-backed serial transport implementing [`AsyncTransport`].
///
/// The const generic `ASCII` selects the framing mode at compile time:
/// - `false` → Modbus RTU (binary, CRC-checked; frame boundary = 3.5-character silence)
/// - `true`  → Modbus ASCII (`:` prefix, `\r\n` terminator, LRC-checked)
///
/// Prefer the type aliases [`TokioRtuTransport`] and [`TokioAsciiTransport`].
///
/// # RTU framing
///
/// `recv()` reads bytes one at a time. After the **first byte arrives**, a
/// `tokio::time::timeout` of `inter_frame_timeout` is reset on every subsequent byte.
/// When the timeout fires (silence detected), the accumulated buffer is returned as a
/// complete frame. Silence before the first byte is ignored — the method suspends until
/// data actually arrives.
///
/// # ASCII framing
///
/// `recv()` accumulates bytes until a `\r\n` sequence is found, then returns the
/// complete frame including the terminator.
#[derive(Debug)]
pub struct TokioSerialTransport<const ASCII: bool = false> {
    port: SerialStream,
    inter_frame_timeout: Duration,
}

/// Modbus RTU serial transport backed by tokio.
pub type TokioRtuTransport = TokioSerialTransport<false>;
/// Modbus ASCII serial transport backed by tokio.
pub type TokioAsciiTransport = TokioSerialTransport<true>;

impl<const ASCII: bool> TokioSerialTransport<ASCII> {
    /// The serial mode determined by the `ASCII` const generic.
    const MODE: SerialMode = if ASCII {
        SerialMode::Ascii
    } else {
        SerialMode::Rtu
    };

    /// Open the serial port described by `config`.
    ///
    /// Returns `Err(MbusError::InvalidConfiguration)` if `config` is not a
    /// `ModbusConfig::Serial` variant or if the mode does not match `ASCII`.
    pub fn new(config: &ModbusConfig) -> Result<Self, MbusError> {
        let serial_cfg = match config {
            ModbusConfig::Serial(c) => c,
            _ => return Err(MbusError::InvalidConfiguration),
        };

        if serial_cfg.mode != Self::MODE {
            return Err(MbusError::InvalidConfiguration);
        }

        let baud_rate: u32 = match serial_cfg.baud_rate {
            BaudRate::Baud9600 => 9600,
            BaudRate::Baud19200 => 19200,
            BaudRate::Custom(r) => r,
        };

        let parity = match serial_cfg.parity {
            Parity::None => tokio_serial::Parity::None,
            Parity::Even => tokio_serial::Parity::Even,
            Parity::Odd => tokio_serial::Parity::Odd,
        };

        let data_bits = match serial_cfg.data_bits {
            DataBits::Five => tokio_serial::DataBits::Five,
            DataBits::Six => tokio_serial::DataBits::Six,
            DataBits::Seven => tokio_serial::DataBits::Seven,
            DataBits::Eight => tokio_serial::DataBits::Eight,
        };

        let stop_bits = match serial_cfg.stop_bits {
            1 => tokio_serial::StopBits::One,
            _ => tokio_serial::StopBits::Two,
        };

        let port = tokio_serial::new(serial_cfg.port_path.as_str(), baud_rate)
            .parity(parity)
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .open_native_async()
            .map_err(|_| MbusError::ConnectionFailed)?;

        let inter_frame_timeout = Self::compute_inter_frame_timeout(baud_rate);

        Ok(Self {
            port,
            inter_frame_timeout,
        })
    }

    /// Compute the 3.5-character-time inter-frame silence, clamped to the spec minimum.
    ///
    /// Formula: `char_time_us = (11 * 1_000_000) / baud_rate` (11 bits per character).
    /// Silence = 3.5 × char_time_us, minimum 1750 µs.
    fn compute_inter_frame_timeout(baud_rate: u32) -> Duration {
        let baud = baud_rate.max(1) as u64;
        let char_time_us = (11 * 1_000_000) / baud;
        let silence_us = ((char_time_us * 7) / 2).max(MIN_INTER_FRAME_US);
        Duration::from_micros(silence_us)
    }

    fn map_io_error(err: std::io::Error) -> MbusError {
        use std::io::ErrorKind::*;
        match err.kind() {
            BrokenPipe | ConnectionReset | UnexpectedEof => MbusError::ConnectionClosed,
            WouldBlock | TimedOut => MbusError::Timeout,
            _ => MbusError::IoError,
        }
    }
}

impl<const ASCII: bool> AsyncTransport for TokioSerialTransport<ASCII> {
    const SUPPORTS_BROADCAST_WRITES: bool = true;
    const TRANSPORT_TYPE: TransportType = TransportType::StdSerial(Self::MODE);

    fn is_connected(&self) -> bool {
        true // serial ports are always "connected" while the port is open
    }

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        self.port.write_all(adu).await.map_err(Self::map_io_error)?;
        self.port.flush().await.map_err(Self::map_io_error)
    }

    async fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        if ASCII {
            self.recv_ascii().await
        } else {
            self.recv_rtu().await
        }
    }
}

impl<const ASCII: bool> TokioSerialTransport<ASCII> {
    /// RTU framing: accumulate bytes; return when inter-frame silence fires.
    async fn recv_rtu(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let mut scratch = [0u8; 1];

        // Wait for the first byte (no timeout — block indefinitely until data arrives)
        self.port.read_exact(&mut scratch).await.map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                MbusError::ConnectionClosed
            } else {
                Self::map_io_error(e)
            }
        })?;
        buf.push(scratch[0])
            .map_err(|_| MbusError::BufferTooSmall)?;

        // Now collect remaining bytes, resetting the silence timer after each one.
        loop {
            match timeout(self.inter_frame_timeout, self.port.read_exact(&mut scratch)).await {
                Ok(Ok(_)) => {
                    buf.push(scratch[0])
                        .map_err(|_| MbusError::BufferTooSmall)?;
                }
                Ok(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    return Err(MbusError::ConnectionClosed);
                }
                Ok(Err(e)) => return Err(Self::map_io_error(e)),
                Err(_elapsed) => {
                    // Silence detected — frame is complete
                    return Ok(buf);
                }
            }
        }
    }

    /// ASCII framing: accumulate bytes until `\r\n` found, then return the frame.
    async fn recv_ascii(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let mut buf: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        let mut scratch = [0u8; 1];

        loop {
            self.port.read_exact(&mut scratch).await.map_err(|e| {
                if e.kind() == std::io::ErrorKind::UnexpectedEof {
                    MbusError::ConnectionClosed
                } else {
                    Self::map_io_error(e)
                }
            })?;

            buf.push(scratch[0])
                .map_err(|_| MbusError::BufferTooSmall)?;

            // ASCII frame ends with CR LF
            let len = buf.len();
            if len >= 2 && buf[len - 2] == b'\r' && buf[len - 1] == b'\n' {
                return Ok(buf);
            }
        }
    }
}
