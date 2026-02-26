use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use mbus_core::transport::tcp::{ModbusTcpTransport, ModbusTcpTransportError};
use heapless::Vec;

/// A concrete implementation of `ModbusTcpTransport` using `std::net::TcpStream`.
///
/// This struct manages a standard TCP connection for Modbus TCP communication.
pub struct StdTcpTransport {
    /// The underlying TCP stream.
    stream: Option<TcpStream>,
    /// The timeout duration for read and write operations.
    timeout: Option<Duration>,
}

impl StdTcpTransport {
    /// Creates a new `StdTcpTransport` instance.
    ///
    /// Initially, there is no active connection.
    ///
    /// # Arguments
    /// * `timeout` - An optional `Duration` for read and write timeouts.
    pub fn new(timeout: Option<Duration>) -> Self {
        Self {
            stream: None,
            timeout,
        }
    }

    /// Helper function to convert `std::io::Error` to `ModbusTcpTransportError`.
    ///
    /// This maps common I/O error kinds to specific Modbus transport errors.
    fn map_io_error(err: io::Error) -> ModbusTcpTransportError {
        match err.kind() {
            io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => ModbusTcpTransportError::ConnectionFailed,
            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset | io::ErrorKind::UnexpectedEof => ModbusTcpTransportError::ConnectionClosed,
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => ModbusTcpTransportError::Timeout,
            _ => ModbusTcpTransportError::IoError,
        }
    }
}

impl ModbusTcpTransport for StdTcpTransport {
    type Error = ModbusTcpTransportError;

    /// Establishes a TCP connection to the specified remote address.
    ///
    /// # Arguments
    /// * `addr` - The address of the Modbus TCP server (e.g., "192.168.1.1:502").
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, addr: &str) -> Result<(), Self::Error> {
        // Resolve the address
        let mut addrs = addr.to_socket_addrs()
            .map_err(|_| ModbusTcpTransportError::ConnectionFailed)?;

        let stream = addrs.next()
            .ok_or(ModbusTcpTransportError::ConnectionFailed)
            .and_then(|addr| {
                TcpStream::connect_timeout(&addr, self.timeout.unwrap_or(Duration::from_secs(5)))
                    .map_err(Self::map_io_error)
            })?;

        stream.set_read_timeout(self.timeout).map_err(Self::map_io_error)?;
        stream.set_write_timeout(self.timeout).map_err(Self::map_io_error)?;
        stream.set_nodelay(true).map_err(Self::map_io_error)?; // Disable Nagle's algorithm for better latency

        self.stream = Some(stream);
        Ok(())
    }

    /// Closes the active TCP connection.
    ///
    /// If no connection is active, this operation does nothing and returns `Ok(())`.
    fn disconnect(&mut self) -> Result<(), Self::Error> {
        // Taking the stream out of the Option will drop it,
        // which in turn closes the underlying TCP connection.
        if let Some(stream) = self.stream.take() {
            drop(stream);
        }
        Ok(())
    }

    /// Sends a Modbus Application Data Unit (ADU) over the TCP connection.
    ///
    /// # Arguments
    /// * `adu` - The byte slice representing the ADU to send.
    ///
    /// # Returns
    /// `Ok(())` if the ADU is successfully sent, or an error otherwise.
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let stream = self.stream.as_mut().ok_or(ModbusTcpTransportError::ConnectionClosed)?;
        stream.write_all(adu).map_err(Self::map_io_error)?;
        stream.flush().map_err(Self::map_io_error)?;
        Ok(())
    }

    /// Receives a Modbus Application Data Unit (ADU) from the TCP connection.
    ///
    /// This method first reads the 7-byte MBAP header to determine the expected
    /// length of the full ADU, then reads the remaining bytes. It ensures that
    /// a complete ADU, as indicated by the MBAP length field, is received.
    ///
    /// # Returns
    /// `Ok(Vec<u8, 260>)` containing the received ADU, or an error otherwise.
    fn recv(&mut self) -> Result<Vec<u8, 260>, Self::Error> {
        let stream = self.stream.as_mut().ok_or(ModbusTcpTransportError::ConnectionClosed)?;
        let mut buffer = Vec::new();
        // Pre-allocate maximum ADU capacity to avoid reallocations.
        buffer.resize(260, 0).map_err(|_| ModbusTcpTransportError::BufferTooSmall)?;

        // 1. Read MBAP header (7 bytes)
        let mut bytes_read_total = 0;
        while bytes_read_total < 7 {
            match stream.read(&mut buffer.as_mut_slice()[bytes_read_total..7]) {
                Ok(0) => return Err(ModbusTcpTransportError::ConnectionClosed), // Peer closed connection
                Ok(n) => bytes_read_total += n,
                Err(e) => return Err(Self::map_io_error(e)),
            }
        }

        // Parse length field from MBAP header (bytes 4 and 5)
        let pdu_and_unit_id_len = u16::from_be_bytes([buffer[4], buffer[5]]);
        let total_adu_len = 6 + pdu_and_unit_id_len as usize; // 6 bytes for TID, PID, Length field itself

        if total_adu_len > 260 {
            return Err(ModbusTcpTransportError::BufferTooSmall); // ADU too large for our buffer
        }

        // 2. Read remaining bytes until the full ADU length is reached
        while bytes_read_total < total_adu_len {
            match stream.read(&mut buffer.as_mut_slice()[bytes_read_total..total_adu_len]) {
                Ok(0) => return Err(ModbusTcpTransportError::ConnectionClosed), // Peer closed connection prematurely
                Ok(n) => bytes_read_total += n,
                Err(e) => return Err(Self::map_io_error(e)),
            }
        }

        buffer.truncate(total_adu_len);
        Ok(buffer)
    }

    /// Checks if the transport is currently connected to a remote host.
    ///
    /// This is a best-effort check and indicates if a `TcpStream` is currently held.
    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::super::std_transport::{StdTcpTransport};
    use mbus_core::transport::tcp::{ModbusTcpTransport, ModbusTcpTransportError};
    use std::io::{self, Read, Write};
    use std::net::{TcpListener};
    use std::time::Duration;
    use std::thread;

    /// Helper function to find an available port.
    fn find_available_port() -> u16 {
        TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port()
    }

    /// Test case: `StdTcpTransport::new` creates an instance with no active connection.
    #[test]
    fn test_new_std_tcp_transport() {
        let transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        assert!(!transport.is_connected());
    }

    /// Test case: `connect` successfully establishes a TCP connection.
    ///
    /// A mock server is set up to accept a single connection.
    #[test]
    fn test_connect_success() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            // Accept one connection and then close
            let _ = listener.accept().unwrap();
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let result = transport.connect(&format!("127.0.0.1:{}", port));
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `connect` fails with an invalid address string.
    #[test]
    fn test_connect_failure_invalid_addr() {
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let result = transport.connect("invalid-address");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionFailed);
        assert!(!transport.is_connected());
    }

    /// Test case: `connect` fails when the server actively refuses the connection.
    ///
    /// This is simulated by trying to connect to a port where no server is listening.
    #[test]
    fn test_connect_failure_connection_refused() {
        let port = find_available_port(); // Get an unused port
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let result = transport.connect(&format!("127.0.0.1:{}", port));
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionFailed);
        assert!(!transport.is_connected());
    }

    /// Test case: `disconnect` closes an active connection.
    #[test]
    fn test_disconnect() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let _ = listener.accept().unwrap(); // Just accept and hold
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();
        assert!(transport.is_connected());

        let result = transport.disconnect();
        assert!(result.is_ok());
        assert!(!transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `send` successfully transmits data over an active connection.
    ///
    /// A mock server receives the data and verifies it.
    #[test]
    fn test_send_success() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        let test_data = [0x01, 0x02, 0x03, 0x04];

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(buf, test_data);
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let result = transport.send(&test_data);
        assert!(result.is_ok());

        server_handle.join().unwrap();
    }

    /// Test case: `send` fails when the transport is not connected.
    #[test]
    fn test_send_failure_not_connected() {
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let test_data = [0x01, 0x02];
        let result = transport.send(&test_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionClosed);
    }

    /// Test case: `recv` successfully receives a complete Modbus ADU.
    ///
    /// A mock server sends a predefined valid ADU.
    #[test]
    fn test_recv_success_full_adu() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        // Example ADU: TID=0x0001, PID=0x0000, Length=0x0003 (Unit ID + FC + 1 data byte), UnitID=0x01, FC=0x03, Data=0x00
        let adu_to_send = [0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x01, 0x03, 0x00];

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&adu_to_send).unwrap();
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let received_adu = transport.recv().unwrap();
        assert_eq!(received_adu.as_slice(), adu_to_send);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` fails when the transport is not connected.
    #[test]
    fn test_recv_failure_not_connected() {
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionClosed);
    }

    /// Test case: `recv` fails when the peer closes the connection prematurely during header read.
    #[test]
    fn test_recv_failure_connection_closed_prematurely_header() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        // Send only part of the MBAP header (e.g., 3 bytes instead of 7)
        let partial_adu = [0x00, 0x01, 0x00];

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&partial_adu).unwrap();
            // Server closes connection after sending partial data
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` fails when the peer closes the connection prematurely after header but before full PDU.
    #[test]
    fn test_recv_failure_connection_closed_prematurely_pdu() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        // Valid MBAP header indicating a PDU length, but then send less than expected
        // TID=0x0001, PID=0x0000, Length=0x0005 (Unit ID + FC + 3 data bytes), UnitID=0x01, FC=0x03
        let partial_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03]; // 8 bytes sent, but 11 expected

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&partial_adu).unwrap();
            // Server closes connection after sending partial PDU data
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` returns `BufferTooSmall` if the ADU length indicated by MBAP header
    /// exceeds the maximum capacity of `Vec<u8, 260>`.
    #[test]
    fn test_recv_failure_buffer_too_small() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        // Craft an ADU header that indicates a length greater than 260 bytes.
        // Max ADU is 260. If length field is 255 (0xFF), total ADU is 6 + 255 = 261.
        let oversized_adu_header = [0x00, 0x01, 0x00, 0x00, 0x00, 0xFF, 0x01]; // Length = 255

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&oversized_adu_header).unwrap();
            // The client should detect the oversized ADU after reading the header
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::BufferTooSmall);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` times out if no data is received within the specified duration.
    #[test]
    fn test_recv_timeout() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (_stream, _) = listener.accept().unwrap();
            // Server accepts connection but sends no data, causing client to timeout
            thread::sleep(Duration::from_secs(5)); // Ensure client times out first
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_millis(100))); // Short timeout for test
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::Timeout);

        server_handle.join().unwrap();
    }

    /// Test case: `is_connected` returns true when connected and false when disconnected.
    #[test]
    fn test_is_connected() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(500)); // Keep connection open briefly
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        assert!(!transport.is_connected());

        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();
        assert!(transport.is_connected());

        transport.disconnect().unwrap();
        assert!(!transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `map_io_error` correctly maps various `io::Error` kinds to `ModbusTcpTransportError`.
    #[test]
    fn test_map_io_error() {
        // ConnectionRefused
        let err = io::Error::new(io::ErrorKind::ConnectionRefused, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::ConnectionFailed);

        // NotFound (often used for address resolution issues)
        let err = io::Error::new(io::ErrorKind::NotFound, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::ConnectionFailed);

        // BrokenPipe
        let err = io::Error::new(io::ErrorKind::BrokenPipe, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::ConnectionClosed);

        // ConnectionReset
        let err = io::Error::new(io::ErrorKind::ConnectionReset, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::ConnectionClosed);

        // UnexpectedEof
        let err = io::Error::new(io::ErrorKind::UnexpectedEof, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::ConnectionClosed);

        // WouldBlock
        let err = io::Error::new(io::ErrorKind::WouldBlock, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::Timeout);

        // TimedOut
        let err = io::Error::new(io::ErrorKind::TimedOut, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::Timeout);

        // Other I/O errors
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), ModbusTcpTransportError::IoError);
    }

    /// Test case: `connect` with a custom timeout.
    #[test]
    fn test_connect_with_custom_timeout() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let _ = listener.accept().unwrap();
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_millis(500))); // Custom timeout
        let result = transport.connect(&format!("127.0.0.1:{}", port));
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `connect` with no timeout specified (uses default).
    #[test]
    fn test_connect_with_no_timeout() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let _ = listener.accept().unwrap();
        });

        let mut transport = StdTcpTransport::new(None); // No timeout
        let result = transport.connect(&format!("127.0.0.1:{}", port));
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `send` fails if the connection is reset by the peer.
    #[test]
    fn test_send_failure_connection_reset() {
        let port = find_available_port();
        let addr = format!("127.0.0.1:{}", port);
        let test_data = [0x01, 0x02, 0x03, 0x04];

        let server_handle = thread::spawn(move || {
            let listener = TcpListener::bind(addr).unwrap();
            let (stream, _) = listener.accept().unwrap();
            drop(stream); // Immediately close the stream after accepting
        });

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        transport.connect(&format!("127.0.0.1:{}", port)).unwrap();

        // Give the server a moment to close the connection
        thread::sleep(Duration::from_millis(100));

        let result = transport.send(&test_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), ModbusTcpTransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }
}
