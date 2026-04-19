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
}

impl TokioTcpTransport {
    /// Wrap an already-accepted [`TcpStream`] as a server-side async transport.
    pub fn from_stream(stream: TcpStream) -> Self {
        Self {
            stream,
            connected: true,
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
    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }

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

        // Step 1: read the 6-byte MBAP prefix (TxnID[2] + ProtocolID[2] + Length[2])
        let mut prefix = [0u8; MBAP_PREFIX_LEN];
        match self.stream.read_exact(&mut prefix).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.connected = false;
                return Err(MbusError::ConnectionClosed);
            }
            Err(e) => {
                let err = Self::map_io_error(e);
                if err == MbusError::ConnectionClosed {
                    self.connected = false;
                }
                return Err(err);
            }
        }

        // Step 2: parse the Length field — number of bytes that follow (unit_id + PDU)
        let remaining_len =
            u16::from_be_bytes([prefix[MBAP_LENGTH_OFFSET_1B], prefix[MBAP_LENGTH_OFFSET_2B]])
                as usize;

        // Guard against malformed or oversized frames
        if MBAP_PREFIX_LEN + remaining_len > MAX_ADU_FRAME_LEN {
            return Err(MbusError::BufferTooSmall);
        }
        if remaining_len == 0 {
            return Err(MbusError::InvalidDataLen);
        }

        // Step 3: read the rest of the frame (unit_id + PDU)
        let mut body = [0u8; MAX_ADU_FRAME_LEN];
        match self.stream.read_exact(&mut body[..remaining_len]).await {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                self.connected = false;
                return Err(MbusError::ConnectionClosed);
            }
            Err(e) => {
                let err = Self::map_io_error(e);
                if err == MbusError::ConnectionClosed {
                    self.connected = false;
                }
                return Err(err);
            }
        }

        // Step 4: assemble prefix + body into a heapless Vec
        let mut frame: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        frame
            .extend_from_slice(&prefix)
            .map_err(|_| MbusError::BufferTooSmall)?;
        frame
            .extend_from_slice(&body[..remaining_len])
            .map_err(|_| MbusError::BufferTooSmall)?;

        Ok(frame)
    }
}
