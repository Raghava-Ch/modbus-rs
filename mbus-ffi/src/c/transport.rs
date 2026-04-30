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

/// C-provided transport callbacks.
///
/// All five callbacks must be non-`NULL` for a valid transport.
/// `userdata` is threaded through every callback invocation.
#[repr(C)]
pub struct MbusTransportCallbacks {
    /// Opaque user context threaded to every callback.
    pub userdata: *mut c_void,
    /// Called to open/connect the transport.
    pub on_connect: Option<unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode>,
    /// Called to close/disconnect the transport.
    pub on_disconnect: Option<unsafe extern "C" fn(userdata: *mut c_void) -> MbusStatusCode>,
    /// Called to send a frame.
    pub on_send: Option<
        unsafe extern "C" fn(data: *const u8, len: u16, userdata: *mut c_void) -> MbusStatusCode,
    >,
    /// Called to receive a frame.
    pub on_recv: Option<
        unsafe extern "C" fn(
            buffer: *mut u8,
            buffer_cap: u16,
            out_len: *mut u16,
            userdata: *mut c_void,
        ) -> MbusStatusCode,
    >,
    /// Called to query connection state.
    pub on_is_connected: Option<unsafe extern "C" fn(userdata: *mut c_void) -> u8>,
}

#[cfg(any(feature = "c", feature = "c-server", feature = "c-gateway"))]
pub(crate) use c_impl::{CAsciiTransport, CRtuTransport, CTcpTransport, validate_transport_callbacks};

/// Implementation types — only compiled when the `c`, `c-server`, or `c-gateway` feature is active.
#[cfg(any(feature = "c", feature = "c-server", feature = "c-gateway"))]
mod c_impl {
    use heapless::Vec;
    use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
    use mbus_core::errors::MbusError;
    use mbus_core::transport::{ModbusConfig, SerialMode, Transport, TransportType};

    use super::MbusTransportCallbacks;
    use crate::c::error::MbusStatusCode;

    pub struct CTcpTransport {
        pub(crate) callbacks: MbusTransportCallbacks,
    }

    impl CTcpTransport {
        pub fn new(callbacks: MbusTransportCallbacks) -> Self {
            Self { callbacks }
        }
    }

    pub struct CSerialTransport<const ASCII: bool = false> {
        pub(crate) callbacks: MbusTransportCallbacks,
    }

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

    pub type CRtuTransport = CSerialTransport<false>;
    pub type CAsciiTransport = CSerialTransport<true>;

    pub fn validate_transport_callbacks(callbacks: &MbusTransportCallbacks) -> bool {
        callbacks.on_connect.is_some()
            && callbacks.on_disconnect.is_some()
            && callbacks.on_send.is_some()
            && callbacks.on_recv.is_some()
            && callbacks.on_is_connected.is_some()
    }

    fn c_connect(callbacks: &MbusTransportCallbacks) -> Result<(), MbusError> {
        let cb = callbacks
            .on_connect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(callbacks.userdata) };
        status_to_result(status)
    }

    fn c_disconnect(callbacks: &MbusTransportCallbacks) -> Result<(), MbusError> {
        let cb = callbacks
            .on_disconnect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(callbacks.userdata) };
        status_to_result(status)
    }

    fn c_send(callbacks: &MbusTransportCallbacks, adu: &[u8]) -> Result<(), MbusError> {
        let cb = callbacks.on_send.ok_or(MbusError::InvalidConfiguration)?;
        let len = u16::try_from(adu.len()).map_err(|_| MbusError::BufferTooSmall)?;
        let status = unsafe { cb(adu.as_ptr(), len, callbacks.userdata) };
        status_to_result(status)
    }

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

    fn c_is_connected(callbacks: &MbusTransportCallbacks) -> bool {
        match callbacks.on_is_connected {
            Some(cb) => unsafe { cb(callbacks.userdata) != 0 },
            None => false,
        }
    }

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

    fn status_to_result(status: MbusStatusCode) -> Result<(), MbusError> {
        if status == MbusStatusCode::MbusOk {
            Ok(())
        } else {
            Err(status_to_error(status))
        }
    }

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
