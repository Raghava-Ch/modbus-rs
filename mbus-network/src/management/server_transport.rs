use heapless::Vec;
use mbus_core::{
    data_unit::common::MAX_ADU_FRAME_LEN,
    transport::{ModbusConfig, Transport, TransportError, TransportType},
};
use std::io::{ErrorKind, Read, Write};
use std::net::{Shutdown, TcpStream};
use std::time::Duration;

/// Transport adapter for an already-accepted TCP connection.
///
/// This is intended for server-side runtimes that accept client sockets via
/// `TcpListener`, then pass each `TcpStream` to Modbus server services.
#[derive(Debug)]
pub struct AcceptedTcpTransport {
    stream: TcpStream,
    connected: bool,
}

impl AcceptedTcpTransport {
    /// Creates a new transport from an accepted TCP stream.
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            connected: true,
        }
    }

    fn map_io_error(err: std::io::Error) -> TransportError {
        match err.kind() {
            ErrorKind::TimedOut | ErrorKind::WouldBlock => TransportError::Timeout,
            ErrorKind::UnexpectedEof
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::BrokenPipe
            | ErrorKind::NotConnected => TransportError::ConnectionClosed,
            _ => TransportError::IoError,
        }
    }
}

impl Transport for AcceptedTcpTransport {
    type Error = TransportError;
    const TRANSPORT_TYPE: Option<TransportType> = Some(TransportType::StdTcp);

    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        let tcp_cfg = match config {
            ModbusConfig::Tcp(v) => v,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        let timeout = Duration::from_millis(tcp_cfg.response_timeout_ms as u64);
        self.stream
            .set_read_timeout(Some(timeout))
            .map_err(Self::map_io_error)?;
        self.stream
            .set_write_timeout(Some(timeout))
            .map_err(Self::map_io_error)?;
        let _ = self.stream.set_nodelay(true);

        self.connected = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        self.connected = false;
        let _ = self.stream.shutdown(Shutdown::Both);
        Ok(())
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        let result = self
            .stream
            .write_all(adu)
            .and_then(|()| self.stream.flush());
        if let Err(err) = result {
            let mapped = Self::map_io_error(err);
            if mapped == TransportError::ConnectionClosed {
                self.connected = false;
            }
            return Err(mapped);
        }

        Ok(())
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        if !self.connected {
            return Err(TransportError::ConnectionClosed);
        }

        self.stream
            .set_nonblocking(true)
            .map_err(Self::map_io_error)?;

        let mut buffer = [0u8; MAX_ADU_FRAME_LEN];
        let read_result = self.stream.read(&mut buffer);
        let _ = self.stream.set_nonblocking(false);

        match read_result {
            Ok(0) => {
                self.connected = false;
                Err(TransportError::ConnectionClosed)
            }
            Ok(n) => Vec::from_slice(&buffer[..n]).map_err(|_| TransportError::BufferTooSmall),
            Err(err) => {
                let mapped = Self::map_io_error(err);
                if mapped == TransportError::ConnectionClosed {
                    self.connected = false;
                }
                Err(mapped)
            }
        }
    }

    fn is_connected(&self) -> bool {
        self.connected
    }

    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }
}
