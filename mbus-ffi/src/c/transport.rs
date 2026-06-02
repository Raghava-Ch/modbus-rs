use core::ffi::c_void;

use super::error::MbusStatusCode;

/// C callback: open / connect the transport.
pub type MbusTransportConnectCb = unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode;
/// C callback: close / disconnect the transport.
pub type MbusTransportDisconnectCb = unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode;
/// C callback: send a frame.
pub type MbusTransportSendCb =
    unsafe extern "C" fn(data: *const u8, len: u16, userdata: *mut c_void) -> MbusStatusCode;
/// C callback: receive a frame.
pub type MbusTransportRecvCb = unsafe extern "C" fn(
    buffer: *mut u8,
    buffer_cap: u16,
    out_len: *mut u16,
    userdata: *mut c_void,
) -> MbusStatusCode;
/// C callback: query connection state.
pub type MbusTransportIsConnectedCb = unsafe extern "C" fn(userdata: *mut c_void) -> u8;

/// C-provided transport callbacks — the only interface between C code and the Rust Modbus stack.
///
/// Fill in this struct and pass it to a `mbus_*_new` constructor.  The Rust stack
/// calls these function pointers whenever it needs to open, close, send data, or
/// receive data over the underlying physical medium (TCP socket, UART, etc.).
///
/// # Requirements
///
/// - **All five callbacks must be non-`NULL`.**  `validate_transport_callbacks` checks
///   this; constructors reject a struct with any `NULL` slot.
/// - Every callback receives the same `userdata` pointer that is stored in this struct.
///   Use it to pass your driver state (file descriptor, hardware register base, etc.).
/// - Callbacks may be called from any context that calls `mbus_*_poll`, `mbus_*_connect`,
///   or `mbus_*_send`.  If you use locking in your driver, acquire and release the lock
///   inside the callback — do **not** hold an external lock while polling.
/// - Callbacks must **never** call back into the Modbus stack (no `mbus_*` functions
///   from inside a callback) — doing so causes re-entrant undefined behaviour.
///
/// # Lifetime
///
/// The struct (and everything `userdata` points to) must remain valid until
/// `mbus_*_free` is called on every server or client that uses it.
///
/// # Minimal example (C, TCP)
///
/// ```c
/// static MbusStatusCode my_connect(void *ud) {
///     MyCtx *ctx = (MyCtx *)ud;
///     ctx->fd = socket_open(ctx->host, ctx->port);
///     return ctx->fd >= 0 ? MBUS_OK : MBUS_ERR_CONNECTION_FAILED;
/// }
/// static MbusStatusCode my_disconnect(void *ud) {
///     socket_close(((MyCtx *)ud)->fd);
///     return MBUS_OK;
/// }
/// static MbusStatusCode my_send(const uint8_t *data, uint16_t len, void *ud) {
///     return socket_write(((MyCtx *)ud)->fd, data, len) == len
///         ? MBUS_OK : MBUS_ERR_SEND_FAILED;
/// }
/// static MbusStatusCode my_recv(uint8_t *buf, uint16_t cap,
///                               uint16_t *out_len, void *ud) {
///     int n = socket_read_nonblocking(((MyCtx *)ud)->fd, buf, cap);
///     if (n > 0) { *out_len = (uint16_t)n; return MBUS_OK; }
///     return n == 0 ? MBUS_ERR_TIMEOUT : MBUS_ERR_IO_ERROR;
/// }
/// static uint8_t my_is_connected(void *ud) {
///     return ((MyCtx *)ud)->fd >= 0 ? 1 : 0;
/// }
///
/// static MbusTransportCallbacks g_transport = {
///     .userdata       = &g_ctx,
///     .on_connect     = my_connect,
///     .on_disconnect  = my_disconnect,
///     .on_send        = my_send,
///     .on_recv        = my_recv,
///     .on_is_connected = my_is_connected,
/// };
/// ```
#[repr(C)]
pub struct MbusTransportCallbacks {
    /// Opaque pointer passed unchanged to every callback invocation.
    ///
    /// Typically points to your driver context struct (file descriptor, register
    /// base address, mutex handle, etc.).  May be `NULL` if your callbacks do not
    /// need per-instance state (e.g. a singleton UART driver).
    pub userdata: *mut c_void,

    /// Open the physical connection.
    ///
    /// Called once by `mbus_*_connect`.  The stack does not call `on_send` or
    /// `on_recv` until this returns `MBUS_OK`.
    ///
    /// # What to do
    /// - Open the socket / serial port / SPI bus.
    /// - Perform any hardware initialisation (baud rate, parity, stop bits).
    /// - For serial servers: configure the UART but do **not** start receiving
    ///   until you return — the stack will call `on_recv` on the first poll.
    ///
    /// # Return values
    /// | Code | Meaning |
    /// |------|---------|
    /// | `MBUS_OK` | Connection established; stack proceeds normally. |
    /// | `MBUS_ERR_CONNECTION_FAILED` | Could not open the port/socket (e.g. ENOENT, EACCES). |
    /// | `MBUS_ERR_IO_ERROR` | Low-level I/O failure during hardware setup. |
    /// | `MBUS_ERR_INVALID_CONFIGURATION` | Supplied config is not supported by this hardware. |
    ///
    /// Any other non-`MBUS_OK` code is mapped to `MbusError::Unexpected` internally.
    pub on_connect: Option<unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode>,

    /// Close the physical connection.
    ///
    /// Called by `mbus_*_disconnect` and during server teardown.  After this
    /// returns, the stack will not call `on_send` or `on_recv` until
    /// `on_connect` is called again.
    ///
    /// # What to do
    /// - Release the socket / serial port file descriptor.
    /// - For RS-485: de-assert the transmit-enable line if your driver holds it.
    /// - This function should be idempotent — calling it on an already-closed
    ///   transport must not crash or return an error.
    ///
    /// # Return values
    /// | Code | Meaning |
    /// |------|---------|
    /// | `MBUS_OK` | Disconnected successfully (or was already disconnected). |
    /// | `MBUS_ERR_IO_ERROR` | An error occurred while flushing or closing the port. |
    ///
    /// The return value is checked but not acted upon by the stack beyond logging.
    pub on_disconnect: Option<unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode>,

    /// Write a complete Modbus ADU frame to the medium.
    ///
    /// The stack calls this once per outgoing frame with a fully-formed byte
    /// slice (`data[0..len]`).  The function must write **all** `len` bytes
    /// atomically from the stack's perspective — partial writes are not retried
    /// at the byte level.
    ///
    /// # Parameters
    /// - `data`     — Pointer to the frame bytes.  Valid only for the duration
    ///   of this call; do not retain it.
    /// - `len`      — Number of bytes to send (always ≤ `MAX_ADU_FRAME_LEN` = 256).
    /// - `userdata` — The value stored in `MbusTransportCallbacks::userdata`.
    ///
    /// # RS-485 / half-duplex requirement
    /// Assert the TX-enable line **before** writing the first byte and
    /// de-assert it **after** the last byte has been shifted out of the UART
    /// FIFO (not just after the write syscall returns).  Failing to do this
    /// corrupts the frame from the perspective of other nodes on the bus.
    ///
    /// # Return values
    /// | Code | Meaning |
    /// |------|---------|
    /// | `MBUS_OK` | All bytes written successfully. |
    /// | `MBUS_ERR_SEND_FAILED` | Write failed partway through (broken pipe, UART overflow). |
    /// | `MBUS_ERR_IO_ERROR` | Hardware-level I/O error (DMA fault, framing error on TX). |
    /// | `MBUS_ERR_CONNECTION_CLOSED` | The remote end closed the connection (TCP only). |
    ///
    /// On failure the stack queues the frame for retry (up to `max_send_retries`).
    pub on_send: Option<
        unsafe extern "C" fn(data: *const u8, len: u16, userdata: *mut c_void) -> MbusStatusCode,
    >,

    /// Read the next available Modbus ADU frame from the medium.
    ///
    /// The stack calls this once per `mbus_*_poll` invocation.  The function
    /// must be **non-blocking** (or use a very short read timeout) so that the
    /// poll loop remains responsive.
    ///
    /// # Parameters
    /// - `buffer`     — Rust-allocated scratch buffer; write received bytes here.
    ///   Valid only for the duration of this call.
    /// - `buffer_cap` — Capacity of `buffer` in bytes (always `MAX_ADU_FRAME_LEN` = 256).
    ///   Never write more than `buffer_cap` bytes.
    /// - `out_len`    — Write the number of bytes placed in `buffer` here.
    ///   Must be set to `0` when returning anything other than `MBUS_OK`.
    /// - `userdata`   — The value stored in `MbusTransportCallbacks::userdata`.
    ///
    /// # TCP / stream transports
    /// Read as many bytes as are available right now (non-blocking).  The Rust
    /// layer uses a sliding window to assemble complete frames from partial reads;
    /// you do not need to wait for a full frame.
    ///
    /// # Serial RTU transports — timing requirements
    /// The `on_recv` callback is the correct place to enforce Modbus
    /// inter-character timing (§2.5 of the Modbus Serial Line Specification):
    ///
    /// - **t3.5 (end-of-frame):** Wait until the line has been silent for at
    ///   least 3.5 character times after the last received byte.  Only then
    ///   return the accumulated bytes with `MBUS_OK`.  This ensures each
    ///   `on_recv` call delivers exactly one complete frame.
    ///
    /// - **t1.5 (framing violation):** If silence of ≥ 1.5 character times is
    ///   detected *between* bytes of the same frame, the frame is corrupt.
    ///   Return **`MBUS_ERR_FRAMING_ERROR`** immediately.  The stack will:
    ///     1. Discard all bytes buffered so far.
    ///     2. Keep the transport connected.
    ///     3. Resume waiting for a clean frame on the next poll.
    ///
    /// # Return values
    /// | Code | Meaning |
    /// |------|---------|
    /// | `MBUS_OK` | Frame bytes written into `buffer`; `*out_len > 0`. |
    /// | `MBUS_ERR_TIMEOUT` | No data available this poll (bus idle / no client request yet). The stack skips processing and polls again. |
    /// | `MBUS_ERR_FRAMING_ERROR` | **Serial only.** t1.5 inter-character gap detected mid-frame. Stack discards the partial frame and continues. |
    /// | `MBUS_ERR_IO_ERROR` | Hardware read error (UART overrun, DMA fault). Stack marks the transport as disconnected. |
    /// | `MBUS_ERR_CONNECTION_CLOSED` | Remote closed the connection (TCP) or the port was unexpectedly closed. Stack disconnects and stops polling. |
    /// | `MBUS_ERR_CONNECTION_LOST` | Connection dropped unexpectedly (e.g. TCP RST). Same effect as `CONNECTION_CLOSED`. |
    /// | `MBUS_ERR_BUFFER_TOO_SMALL` | Frame exceeded `buffer_cap`. Stack discards data (should not happen under normal conditions). |
    ///
    /// **Do not** set `*out_len` to a non-zero value when returning an error code —
    /// the stack ignores `buffer` contents on any non-`MBUS_OK` return.
    pub on_recv: Option<
        unsafe extern "C" fn(
            buffer: *mut u8,
            buffer_cap: u16,
            out_len: *mut u16,
            userdata: *mut c_void,
        ) -> MbusStatusCode,
    >,

    /// Query whether the transport is currently open/connected.
    ///
    /// Called by `mbus_*_is_connected` and internally after a recv/send error
    /// to decide whether to attempt a reconnect.
    ///
    /// # What to do
    /// - Return `1` (non-zero) if the port or socket is open and ready for I/O.
    /// - Return `0` if the transport has been closed, has not yet been opened,
    ///   or is in an error state.
    /// - This function must be **side-effect-free** and **cheap** — it may be
    ///   called multiple times per poll cycle.
    ///
    /// # Return values
    /// | Value | Meaning |
    /// |-------|---------|
    /// | `1` (or any non-zero) | Transport is open and ready. |
    /// | `0` | Transport is closed or in error. |
    pub on_is_connected: Option<unsafe extern "C" fn(userdata: *mut c_void) -> u8>,
}

#[cfg(all(
    any(feature = "c-client", feature = "c-server", feature = "c-gateway"),
    any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    )
))]
pub(crate) use c_impl::validate_transport_callbacks;

#[cfg(feature = "serial-rtu")]
pub(crate) use c_impl::CRtuTransport;

#[cfg(feature = "serial-ascii")]
pub(crate) use c_impl::CAsciiTransport;

#[cfg(feature = "network-tcp")]
pub(crate) use c_impl::CTcpTransport;

#[cfg(any(feature = "c-client", feature = "c-server", feature = "c-gateway"))]
mod c_impl {
    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use heapless::Vec;
    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use mbus_core::errors::MbusError;
    #[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
    use mbus_core::transport::SerialMode;
    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use mbus_core::transport::{ModbusConfig, Transport, TransportType};

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use super::MbusTransportCallbacks;
    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    use crate::c::error::MbusStatusCode;

    #[cfg(feature = "network-tcp")]
    pub struct CTcpTransport {
        pub(crate) callbacks: MbusTransportCallbacks,
    }

    #[cfg(feature = "network-tcp")]
    impl CTcpTransport {
        pub fn new(callbacks: MbusTransportCallbacks) -> Self {
            Self { callbacks }
        }
    }

    #[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
    pub struct CSerialTransport<const ASCII: bool = false> {
        pub(crate) callbacks: MbusTransportCallbacks,
    }

    #[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
    impl<const ASCII: bool> CSerialTransport<ASCII> {
        pub const MODE: SerialMode = if ASCII {
            SerialMode::Ascii
        } else {
            SerialMode::Rtu
        };

        pub fn new(callbacks: MbusTransportCallbacks) -> Self {
            Self { callbacks }
        }
    }

    #[cfg(feature = "serial-rtu")]
    pub type CRtuTransport = CSerialTransport<false>;
    #[cfg(feature = "serial-ascii")]
    pub type CAsciiTransport = CSerialTransport<true>;

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    pub fn validate_transport_callbacks(callbacks: &MbusTransportCallbacks) -> bool {
        callbacks.on_connect.is_some()
            && callbacks.on_disconnect.is_some()
            && callbacks.on_send.is_some()
            && callbacks.on_recv.is_some()
            && callbacks.on_is_connected.is_some()
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn c_connect(callbacks: &MbusTransportCallbacks) -> Result<(), MbusError> {
        let cb = callbacks
            .on_connect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(callbacks.userdata) };
        status_to_result(status)
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn c_disconnect(callbacks: &MbusTransportCallbacks) -> Result<(), MbusError> {
        let cb = callbacks
            .on_disconnect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(callbacks.userdata) };
        status_to_result(status)
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn c_send(callbacks: &MbusTransportCallbacks, adu: &[u8]) -> Result<(), MbusError> {
        let cb = callbacks.on_send.ok_or(MbusError::InvalidConfiguration)?;
        let len = u16::try_from(adu.len()).map_err(|_| MbusError::BufferTooSmall)?;
        let status = unsafe { cb(adu.as_ptr(), len, callbacks.userdata) };
        status_to_result(status)
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn c_recv(callbacks: &MbusTransportCallbacks) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, MbusError> {
        let cb = callbacks.on_recv.ok_or(MbusError::InvalidConfiguration)?;

        let mut buf = [0u8; MAX_ADU_FRAME_LEN];
        let mut out_len: u16 = 0;
        let cap = u16::try_from(MAX_ADU_FRAME_LEN).map_err(|_| MbusError::BufferTooSmall)?;

        let status = unsafe {
            cb(
                buf.as_mut_ptr(),
                cap,
                &mut out_len as *mut u16,
                callbacks.userdata,
            )
        };
        if status != MbusStatusCode::MbusOk {
            return Err(status_to_error(status));
        }

        let out_len_usize = out_len as usize;
        if out_len_usize == 0 {
            return Err(MbusError::Timeout);
        }
        if out_len_usize > MAX_ADU_FRAME_LEN {
            return Err(MbusError::BufferTooSmall);
        }

        let mut out: Vec<u8, MAX_ADU_FRAME_LEN> = Vec::new();
        out.extend_from_slice(&buf[..out_len_usize])
            .map_err(|_| MbusError::BufferTooSmall)?;
        Ok(out)
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn c_is_connected(callbacks: &MbusTransportCallbacks) -> bool {
        match callbacks.on_is_connected {
            Some(cb) => unsafe { cb(callbacks.userdata) != 0 },
            None => false,
        }
    }

    #[cfg(feature = "network-tcp")]
    impl Transport for CTcpTransport {
        type Error = MbusError;
        const TRANSPORT_TYPE: TransportType = TransportType::CustomTcp;

        fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
            c_connect(&self.callbacks)
        }

        fn disconnect(&mut self) -> Result<(), Self::Error> {
            c_disconnect(&self.callbacks)
        }

        fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
            c_send(&self.callbacks, adu)
        }

        fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
            c_recv(&self.callbacks)
        }

        fn is_connected(&self) -> bool {
            c_is_connected(&self.callbacks)
        }
    }

    #[cfg(any(feature = "serial-rtu", feature = "serial-ascii"))]
    impl<const ASCII: bool> Transport for CSerialTransport<ASCII> {
        type Error = MbusError;
        const SUPPORTS_BROADCAST_WRITES: bool = true;
        const TRANSPORT_TYPE: TransportType = TransportType::CustomSerial(Self::MODE);

        fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
            c_connect(&self.callbacks)
        }

        fn disconnect(&mut self) -> Result<(), Self::Error> {
            c_disconnect(&self.callbacks)
        }

        fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
            c_send(&self.callbacks, adu)
        }

        fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
            c_recv(&self.callbacks)
        }

        fn is_connected(&self) -> bool {
            c_is_connected(&self.callbacks)
        }
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn status_to_result(status: MbusStatusCode) -> Result<(), MbusError> {
        if status == MbusStatusCode::MbusOk {
            Ok(())
        } else {
            Err(status_to_error(status))
        }
    }

    #[cfg(any(
        feature = "network-tcp",
        feature = "serial-rtu",
        feature = "serial-ascii"
    ))]
    fn status_to_error(status: MbusStatusCode) -> MbusError {
        match status {
            MbusStatusCode::MbusErrParseError => MbusError::ParseError,
            MbusStatusCode::MbusErrBasicParseError => MbusError::BasicParseError,
            MbusStatusCode::MbusErrTimeout => MbusError::Timeout,
            MbusStatusCode::MbusErrIoError => MbusError::IoError,
            MbusStatusCode::MbusErrConnectionFailed => MbusError::ConnectionFailed,
            MbusStatusCode::MbusErrConnectionClosed => MbusError::ConnectionClosed,
            MbusStatusCode::MbusErrConnectionLost => MbusError::ConnectionLost,
            MbusStatusCode::MbusErrBufferTooSmall => MbusError::BufferTooSmall,
            MbusStatusCode::MbusErrSendFailed => MbusError::SendFailed,
            MbusStatusCode::MbusErrInvalidConfiguration => MbusError::InvalidConfiguration,
            MbusStatusCode::MbusErrInvalidTransport => MbusError::InvalidTransport,
            _ => MbusError::Unexpected,
        }
    }
}
