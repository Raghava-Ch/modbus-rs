use std::io::{self, Read, Write};
use std::time::Duration;

use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::transport::{
    BaudRate, DataBits as ConfigDataBits, ModbusConfig, Parity, SerialMode, Transport,
    TransportError, TransportType,
};
use serialport::{ClearBuffer, DataBits as SerialPortDataBits, FlowControl, SerialPort, StopBits};

/// A concrete implementation of `Transport` for Serial communication using `serialport` crate.
/// Supports both RTU and ASCII modes.
#[derive(Debug)]
pub struct StdSerialTransport {
    port: Option<Box<dyn SerialPort>>,
    mode: SerialMode, // The serial mode (RTU or ASCII).
    // Store the configured timeout to restore it after dynamic adjustments in recv
    timeout: Duration,
    // Store the baud rate to calculate inter-frame delays dynamically.
    baud_rate: u32,
}

impl StdSerialTransport {
    /// Creates a new `StdSerialTransport` instance.
    pub fn new(mode: SerialMode) -> Self {
        Self {
            port: None,
            mode,
            timeout: Duration::from_secs(1), // Default safe value, overwritten in connect
            baud_rate: 9600,                 // Default, overwritten in connect.
        }
    }

    /// Returns a list of available serial ports on the system.
    /// This can be useful for allowing a user to select a port.
    pub fn available_ports() -> Result<std::vec::Vec<serialport::SerialPortInfo>, serialport::Error>
    {
        serialport::available_ports()
    }

    /// Helper function to convert `std::io::Error` to `TransportError`.
    ///
    /// This maps common I/O error kinds to specific Modbus transport errors.
    fn map_io_error(err: io::Error) -> TransportError {
        match err.kind() {
            io::ErrorKind::TimedOut => TransportError::Timeout,
            io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::UnexpectedEof => TransportError::ConnectionClosed,
            _ => TransportError::IoError,
        }
    }
}

impl Transport for StdSerialTransport {
    type Error = TransportError;

    /// Establishes a connection to the specified serial port.
    ///
    /// # Arguments
    /// * `config` - The `ModbusConfig` containing the serial port configuration.
    ///   This must be the `ModbusConfig::Serial` variant.
    ///
    /// # Returns
    /// `Ok(())` if the connection is successfully established, or an error otherwise.
    fn connect(&mut self, config: &ModbusConfig) -> Result<(), Self::Error> {
        let serial_config = match config {
            ModbusConfig::Serial(c) => c,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        // Ensure the mode from the configuration matches the mode this transport was initialized with.
        if serial_config.mode != self.mode {
            return Err(TransportError::InvalidConfiguration);
        }

        self.baud_rate = match serial_config.baud_rate {
            BaudRate::Baud9600 => 9600,
            BaudRate::Baud19200 => 19200,
            BaudRate::Custom(rate) => rate,
        };

        let parity = match serial_config.parity {
            Parity::None => serialport::Parity::None,
            Parity::Even => serialport::Parity::Even,
            Parity::Odd => serialport::Parity::Odd,
        };

        let data_bits = match serial_config.data_bits {
            ConfigDataBits::Five => SerialPortDataBits::Five,
            ConfigDataBits::Six => SerialPortDataBits::Six,
            ConfigDataBits::Seven => SerialPortDataBits::Seven,
            ConfigDataBits::Eight => SerialPortDataBits::Eight,
        };

        // Convert the numeric stop_bits from config to the serialport enum.
        let stop_bits = match serial_config.stop_bits {
            1 => StopBits::One,
            2 => StopBits::Two,
            _ => return Err(TransportError::InvalidConfiguration),
        };

        self.timeout = Duration::from_millis(serial_config.response_timeout_ms as u64);

        // Build the serial port configuration.
        let builder = serialport::new(serial_config.port_path.as_str(), self.baud_rate)
            .parity(parity)
            .data_bits(data_bits)
            .stop_bits(stop_bits) // Use stop_bits from config.
            .flow_control(FlowControl::None)
            .timeout(self.timeout);

        // Attempt to open the port.
        match builder.open() {
            Ok(port) => {
                if let Err(e) = port.clear(ClearBuffer::All) {
                    eprintln!("Warning: Failed to clear serial buffers on connect: {}", e);
                }
                self.port = Some(port);
                Ok(())
            }
            Err(e) => {
                eprintln!(
                    "Failed to open serial port '{}': {}",
                    serial_config.port_path.as_str(),
                    e
                );
                // Provide platform-specific hints for common serial port errors.
                #[cfg(windows)]
                {
                    let error_string = e.to_string().to_lowercase();
                    if error_string.contains("access is denied") {
                        eprintln!(
                            "Hint: 'Access is denied' on Windows usually means the port is already in use by another application."
                        );
                    }
                    if error_string.contains("the system cannot find the file specified") {
                        eprintln!(
                            "Hint: 'The system cannot find the file specified' on Windows means the port does not exist. Check available ports."
                        );
                    }
                }
                if e.to_string().contains("Not a typewriter") {
                    eprintln!(
                        "Hint: This error often occurs on macOS when using a pseudo-terminal (pty) created by tools like socat."
                    );
                    eprintln!(
                        "PTYs may not support setting serial parameters like baud rate. Consider using a physical serial port or a different virtual setup."
                    );
                }
                Err(TransportError::ConnectionFailed)
            }
        }
    }

    /// Closes the active serial port connection.
    ///
    /// If no connection is active, this operation does nothing and returns `Ok(())`.
    fn disconnect(&mut self) -> Result<(), Self::Error> {
        // Dropping the `port` will automatically close the serial connection.
        self.port = None;
        Ok(())
    }

    /// Sends a Modbus Application Data Unit (ADU) over the serial port.
    ///
    /// # Arguments
    /// * `adu` - The byte slice representing the ADU to send.
    ///
    /// # Returns
    /// `Ok(())` if the ADU is successfully sent, or an error otherwise.
    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let port = self.port.as_mut().ok_or(TransportError::ConnectionClosed)?;

        // Before sending a new request, it's crucial to clear any data
        // that may have been left in the buffers from a previous, possibly incomplete,
        // transaction. This prevents stale data from being misinterpreted as a response
        // to the new request.
        if let Err(e) = port.clear(ClearBuffer::All) {
            eprintln!("Warning: Failed to clear serial buffers before send: {}", e);
            // This is often not a fatal error, so we log it and continue.
        }

        port.write_all(adu).map_err(|e| {
            eprintln!("Serial write_all failed: {}", e);
            Self::map_io_error(e)
        })?;

        match port.flush() {
            Ok(_) => Ok(()),
            Err(e) => {
                // On Windows, some drivers (e.g. some USB-to-Serial) return "Incorrect function" (OS error 1)
                // when FlushFileBuffers is called. Since write_all succeeded, we can often ignore this.
                #[cfg(windows)]
                if let Some(1) = e.raw_os_error() {
                    // Ignoring this specific error is a workaround for buggy drivers.
                    return Ok(());
                }
                eprintln!("Serial flush failed: {}", e);
                Err(Self::map_io_error(e))
            }
        }
    }

    /// Receives a Modbus Application Data Unit (ADU) from the serial port.
    ///
    /// This implementation is non-blocking: it checks the serial port's input buffer
    /// and reads only the bytes currently available. If no bytes are available,
    /// it returns an empty `Vec`.
    ///
    /// # Returns
    /// `Ok(Vec<u8, 260>)` containing the received ADU, or an error otherwise.
    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        let port = self.port.as_mut().ok_or(TransportError::ConnectionClosed)?;

        // Check how many bytes are available in the RX buffer to ensure non-blocking behavior.
        let bytes_to_read = port.bytes_to_read().map_err(|e| {
            eprintln!("Failed to check available bytes: {}", e);
            TransportError::IoError
        })?;

        let mut buffer = Vec::new();

        if bytes_to_read > 0 {
            // Limit the read to the capacity of our heapless::Vec (260 bytes for Modbus ADU).
            let limit = std::cmp::min(bytes_to_read as usize, buffer.capacity());

            // Create a temporary slice to read into.
            let mut temp_buf = [0u8; MAX_ADU_FRAME_LEN];
            let read_count = port.read(&mut temp_buf[..limit]).map_err(|e| {
                if e.kind() == io::ErrorKind::WouldBlock {
                    return TransportError::IoError; // Or handle as empty if preferred
                }
                Self::map_io_error(e)
            })?;

            // Extend the heapless Vec with the bytes actually read.
            if buffer.extend_from_slice(&temp_buf[..read_count]).is_err() {
                return Err(TransportError::IoError); // Should not happen given the limit check.
            }
        }

        Ok(buffer)
    }

    /// Checks if the transport is currently connected to a remote host.
    fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    /// Returns the type of transport.
    fn transport_type(&self) -> TransportType {
        let mode = self.mode; // SerialMode implements Copy, so no need to clone
        TransportType::StdSerial(mode)
    }
}
