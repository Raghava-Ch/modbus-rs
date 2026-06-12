use heapless::Vec;
use mbus_core::data_unit::common::{
    MAX_ADU_FRAME_LEN, MBAP_LENGTH_OFFSET_1B, MBAP_LENGTH_OFFSET_2B,
};
use mbus_core::errors::MbusError;
use mbus_core::transport::{AsyncTransport, TransportType};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

/// Number of bytes in the MBAP prefix read before the length field.
///
/// The Modbus TCP ADU begins with a 6-byte "prefix":
/// `[TxnID(2), ProtocolID(2), Length(2)]`. Reading these 6 bytes first allows
/// us to determine the total remaining frame length before issuing a second read.
const MBAP_PREFIX_LEN: usize = 6;

/// Tokio-backed TCP transport implementing [`AsyncTransport`].
///
/// Created via [`TokioTcpTransport::from_stream`] for server-side use (wrapping an
/// already-accepted [`TcpStream`]), or via [`TokioTcpTransport::connect`] for
/// future client-side use.
///
/// `recv()` reads the 6-byte MBAP prefix, parses the length field, then reads exactly
/// the remaining bytes — always returning a single complete Modbus TCP ADU frame.
#[derive(Debug)]
pub struct TokioTcpTransport {
    stream: TcpStream,
    connected: bool,
    rx_buf: std::vec::Vec<u8>,
}

impl TokioTcpTransport {
    /// Wrap an already-accepted [`TcpStream`] as a server-side async transport.
    pub fn from_stream(stream: TcpStream) -> Self {
        Self {
            stream,
            connected: true,
            rx_buf: std::vec::Vec::with_capacity(2 * MAX_ADU_FRAME_LEN),
        }
    }

    /// Dial out to a remote address, returning a connected async transport.
    ///
    /// This is the future client path. Currently used by `mbus-async` server
    /// integration tests that need a loopback connection.
    pub async fn connect(addr: impl tokio::net::ToSocketAddrs) -> Result<Self, MbusError> {
        let stream = TcpStream::connect(addr)
            .await
            .map_err(|_| MbusError::ConnectionFailed)?;
        let _ = stream.set_nodelay(true);
        Ok(Self {
            stream,
            connected: true,
            rx_buf: std::vec::Vec::with_capacity(2 * MAX_ADU_FRAME_LEN),
        })
    }

    fn map_io_error(err: std::io::Error) -> MbusError {
        use std::io::ErrorKind::*;
        match err.kind() {
            ConnectionRefused | NotFound => MbusError::ConnectionFailed,
            BrokenPipe | ConnectionReset | ConnectionAborted | UnexpectedEof => {
                MbusError::ConnectionClosed
            }
            WouldBlock | TimedOut => MbusError::Timeout,
            _ => MbusError::IoError,
        }
    }
}

impl AsyncTransport for TokioTcpTransport {
    const SUPPORTS_BROADCAST_WRITES: bool = false;
    const TRANSPORT_TYPE: TransportType = TransportType::StdTcp;

    fn is_connected(&self) -> bool {
        self.connected
    }

    async fn send(&mut self, adu: &[u8]) -> Result<(), MbusError> {
        if !self.connected {
            return Err(MbusError::ConnectionClosed);
        }
        self.stream.write_all(adu).await.map_err(|e| {
            let err = Self::map_io_error(e);
            if err == MbusError::ConnectionClosed {
                self.connected = false;
            }
            err
        })?;
        self.stream.flush().await.map_err(Self::map_io_error)
    }

    async fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        if !self.connected {
            return Err(MbusError::ConnectionClosed);
        }

        loop {
            // Step 1: check if we already have a complete frame in rx_buf
            if self.rx_buf.len() >= MBAP_PREFIX_LEN {
                let remaining_len = u16::from_be_bytes([
                    self.rx_buf[MBAP_LENGTH_OFFSET_1B],
                    self.rx_buf[MBAP_LENGTH_OFFSET_2B],
                ]) as usize;

                if remaining_len == 0 {
                    self.rx_buf.clear();
                    return Err(MbusError::InvalidDataLen);
                }

                let total_len = MBAP_PREFIX_LEN + remaining_len;
                if total_len > MAX_ADU_FRAME_LEN {
                    self.rx_buf.clear();
                    return Err(MbusError::BufferTooSmall);
                }

                if self.rx_buf.len() >= total_len {
                    // We have a complete frame! Extract it.
                    let mut frame = Vec::new();
                    frame.extend_from_slice(&self.rx_buf[..total_len]).unwrap();

                    // Remove the extracted bytes from rx_buf
                    let leftover = self.rx_buf.len() - total_len;
                    if leftover > 0 {
                        // Shift remaining bytes to the front
                        self.rx_buf.copy_within(total_len.., 0);
                    }
                    self.rx_buf.truncate(leftover);

                    return Ok(frame);
                }
            }

            // Step 2: read more bytes from the stream
            let mut chunk = [0u8; 128];
            match self.stream.read(&mut chunk).await {
                Ok(0) => {
                    self.connected = false;
                    return Err(MbusError::ConnectionClosed);
                }
                Ok(n) => {
                    // Append read bytes to rx_buf. If it doesn't fit, it's an error.
                    if self.rx_buf.len() + n > 2 * MAX_ADU_FRAME_LEN {
                        self.rx_buf.clear();
                        return Err(MbusError::BufferTooSmall);
                    }
                    self.rx_buf.extend_from_slice(&chunk[..n]);
                }
                Err(e) => {
                    let err = Self::map_io_error(e);
                    if err == MbusError::ConnectionClosed {
                        self.connected = false;
                    }
                    return Err(err);
                }
            }
        }
    }
}
