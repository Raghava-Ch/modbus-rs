use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use mbus_core::transport::{Transport, TransportError, TransportType, ModbusTcpConfig};
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

    /// Helper function to convert `std::io::Error` to `TransportError`.
    ///
    /// This maps common I/O error kinds to specific Modbus transport errors.
    fn map_io_error(err: io::Error) -> TransportError {
        match err.kind() {
            io::ErrorKind::ConnectionRefused | io::ErrorKind::NotFound => TransportError::ConnectionFailed,
            io::ErrorKind::BrokenPipe | io::ErrorKind::ConnectionReset | io::ErrorKind::UnexpectedEof => TransportError::ConnectionClosed,
            io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut => TransportError::Timeout,
            _ => TransportError::IoError,
        }
    }
}

impl Transport for StdTcpTransport {
    type Error = TransportError;

    /// Establishes a TCP connection to the specified remote address.
    ///
    /// # Arguments
    /// * `addr` - The address of the Modbus TCP server (e.g., "192.168.1.1:502").
    /// * `config` - The `ModbusTcpConfig` containing the host and port of the Modbus TCP server.
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, config: &ModbusTcpConfig) -> Result<(), Self::Error> {
        // Resolve the address
        let mut addrs = format!("{}:{}", config.host, config.port).to_socket_addrs()
            .map_err(|_| TransportError::ConnectionFailed)?;

        let stream = addrs.next()
            .ok_or(TransportError::ConnectionFailed)
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
        let stream = self.stream.as_mut().ok_or(TransportError::ConnectionClosed)?;
        
        let result = stream.write_all(adu).and_then(|()| stream.flush());

        if let Err(err) = result {
            let transport_error = Self::map_io_error(err);
            if transport_error == TransportError::ConnectionClosed {
                self.stream = None;
            }
            return Err(transport_error);
        }

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
        let stream = self.stream.as_mut().ok_or(TransportError::ConnectionClosed)?;
        
        // Helper closure to handle errors and update state
        let handle_error = |err: TransportError, stream_opt: &mut Option<TcpStream>| {
            if err == TransportError::ConnectionClosed {
                *stream_opt = None;
            }
            err
        };

        let mut buffer = Vec::new();
        buffer.resize(260, 0).map_err(|_| TransportError::BufferTooSmall)?;

        // 1. Read MBAP header (7 bytes)
        let mut bytes_read_total = 0;
        while bytes_read_total < 7 {
            match stream.read(&mut buffer.as_mut_slice()[bytes_read_total..7]) {
                Ok(0) => return Err(handle_error(TransportError::ConnectionClosed, &mut self.stream)),
                Ok(n) => bytes_read_total += n,
                Err(e) => return Err(handle_error(Self::map_io_error(e), &mut self.stream)),
            }
        }

        // Parse length field
        let pdu_and_unit_id_len = u16::from_be_bytes([buffer[4], buffer[5]]);
        let total_adu_len = 6 + pdu_and_unit_id_len as usize;

        if total_adu_len > 260 {
            return Err(TransportError::BufferTooSmall);
        }

        // 2. Read remaining bytes
        while bytes_read_total < total_adu_len {
            match stream.read(&mut buffer.as_mut_slice()[bytes_read_total..total_adu_len]) {
                Ok(0) => return Err(handle_error(TransportError::ConnectionClosed, &mut self.stream)),
                Ok(n) => bytes_read_total += n,
                Err(e) => return Err(handle_error(Self::map_io_error(e), &mut self.stream)),
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

    /// Returns the type of transport.
    fn transport_type(&self) -> TransportType {
        TransportType::StdTcp
    }
}

#[cfg(test)]
impl StdTcpTransport {
    pub fn stream_mut(&mut self) -> Option<&mut TcpStream> {
        self.stream.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::super::std_transport::{StdTcpTransport};
    use mbus_core::transport::{ModbusTcpConfig, Transport, TransportError};
    use std::io::{self, Read, Write};
    use std::net::{TcpListener};
    use std::time::Duration;
    use std::thread;
    use std::sync::mpsc;

    /// Helper function to create a TcpListener on an available port.
    /// This listener is then passed to the server thread.
    fn create_test_listener() -> TcpListener {
        TcpListener::bind("127.0.0.1:0").expect("Failed to bind to an available port")
    }

    /// Helper function to extract host and port from a SocketAddr.
    fn get_host_port(addr: std::net::SocketAddr) -> u16 {
        addr.port()
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
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal"); // Signal that the listener is ready
            // Accept one connection and then close
            let _ = listener.accept().unwrap();
        });

        rx.recv().expect("Failed to receive server ready signal"); // Wait for the server to be ready

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        let result = transport.connect(&config);
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `connect` fails with an invalid address string.
    #[test]
    fn test_connect_failure_invalid_addr() {
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let config = ModbusTcpConfig::new("invalid-address", 502); // Invalid host
        let result = transport.connect(&config);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::ConnectionFailed);
        assert!(!transport.is_connected());
    }

    /// Test case: `connect` fails when the server actively refuses the connection.
    ///
    /// This is simulated by trying to connect to a port where no server is listening.
    #[test]
    fn test_connect_failure_connection_refused() {
        // We don't start a server, so the port will be refused
        let listener = create_test_listener(); // Just to get an unused port
        let port = listener.local_addr().unwrap().port();
        drop(listener); // Explicitly drop the listener to ensure the port is free
        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        let result = transport.connect(&config);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::ConnectionFailed);
        assert!(!transport.is_connected());
    }

    /// Test case: `disconnect` closes an active connection.
    #[test]
    fn test_disconnect() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let _ = listener.accept().unwrap(); // Just accept and hold
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();
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
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let test_data = [0x01, 0x02, 0x03, 0x04];

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (mut stream, _) = listener.accept().unwrap();
            let mut buf = [0; 4];
            stream.read_exact(&mut buf).unwrap();
            assert_eq!(buf, test_data);
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

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
        assert_eq!(result.unwrap_err(), TransportError::ConnectionClosed);
    }

    /// Test case: `recv` successfully receives a complete Modbus ADU.
    ///
    /// A mock server sends a predefined valid ADU.
    #[test]
    fn test_recv_success_full_adu() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        // Example ADU: TID=0x0001, PID=0x0000, Length=0x0003 (Unit ID + FC + 1 data byte), UnitID=0x01, FC=0x03, Data=0x00
        let adu_to_send = [0x00, 0x01, 0x00, 0x00, 0x00, 0x03, 0x01, 0x03, 0x00];

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&adu_to_send).unwrap();
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);

        transport.connect(&config).unwrap();

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
        assert_eq!(result.unwrap_err(), TransportError::ConnectionClosed);
    }

    /// Test case: `recv` fails when the peer closes the connection prematurely during header read.
    #[test]
    fn test_recv_failure_connection_closed_prematurely_header() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        // Send only part of the MBAP header (e.g., 3 bytes instead of 7)
        let partial_adu = [0x00, 0x01, 0x00];

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&partial_adu).unwrap();
            // Server closes connection after sending partial data
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` fails when the peer closes the connection prematurely after header but before full PDU.
    #[test]
    fn test_recv_failure_connection_closed_prematurely_pdu() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        // Valid MBAP header indicating a PDU length, but then send less than expected
        // TID=0x0001, PID=0x0000, Length=0x0005 (Unit ID + FC + 3 data bytes), UnitID=0x01, FC=0x03
        let partial_adu = [0x00, 0x01, 0x00, 0x00, 0x00, 0x05, 0x01, 0x03]; // 8 bytes sent, but 11 expected

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&partial_adu).unwrap();
            // Server closes connection after sending partial PDU data
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` returns `BufferTooSmall` if the ADU length indicated by MBAP header
    /// exceeds the maximum capacity of `Vec<u8, 260>`.
    #[test]
    fn test_recv_failure_buffer_too_small() { // Corrected function name
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        // Craft an ADU header that indicates a length greater than 260 bytes.
        // Max ADU is 260. If length field is 255 (0xFF), total ADU is 6 + 255 = 261.
        let oversized_adu_header = [0x00, 0x01, 0x00, 0x00, 0x00, 0xFF, 0x01]; // Length = 255

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (mut stream, _) = listener.accept().unwrap();
            stream.write_all(&oversized_adu_header).unwrap();
            // The client should detect the oversized ADU after reading the header
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::BufferTooSmall);

        server_handle.join().unwrap();
    }

    /// Test case: `recv` times out if no data is received within the specified duration.
    #[test]
    fn test_recv_timeout() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (_stream, _) = listener.accept().unwrap();
            // Server accepts connection but sends no data, causing client to timeout
            thread::sleep(Duration::from_secs(5)); // Ensure client times out first
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_millis(100))); // Very short timeout for test
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

        let result = transport.recv();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::Timeout);

        server_handle.join().unwrap();
    }

    /// Test case: `is_connected` returns true when connected and false when disconnected.
    #[test]
    fn test_is_connected() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(500)); // Keep connection open briefly
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        assert!(!transport.is_connected());

        let config = ModbusTcpConfig::new("127.0.0.1", port);
        transport.connect(&config).unwrap();

        assert!(transport.is_connected());

        transport.disconnect().unwrap();
        assert!(!transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `map_io_error` correctly maps various `io::Error` kinds to `TransportError`.
    #[test]
    fn test_map_io_error() {
        // ConnectionRefused
        let err = io::Error::new(io::ErrorKind::ConnectionRefused, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::ConnectionFailed);

        // NotFound (often used for address resolution issues)
        let err = io::Error::new(io::ErrorKind::NotFound, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::ConnectionFailed);

        // BrokenPipe
        let err = io::Error::new(io::ErrorKind::BrokenPipe, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::ConnectionClosed);

        // ConnectionReset
        let err = io::Error::new(io::ErrorKind::ConnectionReset, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::ConnectionClosed);

        // UnexpectedEof
        let err = io::Error::new(io::ErrorKind::UnexpectedEof, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::ConnectionClosed);

        // WouldBlock
        let err = io::Error::new(io::ErrorKind::WouldBlock, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::Timeout);

        // TimedOut
        let err = io::Error::new(io::ErrorKind::TimedOut, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::Timeout);

        // Other I/O errors
        let err = io::Error::new(io::ErrorKind::PermissionDenied, "test");
        assert_eq!(StdTcpTransport::map_io_error(err), TransportError::IoError);
    }

    /// Test case: `connect` with a custom timeout.
    #[test]
    fn test_connect_with_custom_timeout() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let _ = listener.accept().unwrap();
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_millis(500))); // Custom timeout
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        let result = transport.connect(&config);
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `connect` with no timeout specified (uses default).
    #[test]
    fn test_connect_with_no_timeout() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let _ = listener.accept().unwrap();
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(None); // No timeout
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);
        let result = transport.connect(&config);
        assert!(result.is_ok());
        assert!(transport.is_connected());

        server_handle.join().unwrap();
    }

    /// Test case: `send` fails if the connection is reset by the peer.
    #[test]
    fn test_send_failure_connection_reset() {
        let listener = create_test_listener();
        let addr = listener.local_addr().unwrap();
        let (tx, rx) = mpsc::channel();
        let test_data = [0x01, 0x02, 0x03, 0x04];

        let server_handle = thread::spawn(move || {
            tx.send(()).expect("Failed to send server ready signal");
            let (stream, _) = listener.accept().unwrap();
            drop(stream); // Immediately close the stream after accepting
        });

        rx.recv().expect("Failed to receive server ready signal");

        let mut transport = StdTcpTransport::new(Some(Duration::from_secs(1)));
        let port = get_host_port(addr);
        let config = ModbusTcpConfig::new("127.0.0.1", port);

        transport.connect(&config).unwrap();

        assert!(transport.is_connected());

        // Attempt a receive operation to force the client's TcpStream to detect the peer's closure.
        // This should result in TransportError::ConnectionClosed and update the transport's state.
        let recv_result = transport.recv();
        assert!(recv_result.is_err());
        assert_eq!(recv_result.unwrap_err(), TransportError::ConnectionClosed);
        // Now, the transport should report as disconnected.
        assert!(!transport.is_connected());

        // A subsequent send operation should now reliably fail with ConnectionClosed.
        let result = transport.send(&test_data);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), TransportError::ConnectionClosed);

        server_handle.join().unwrap();
    }
}
