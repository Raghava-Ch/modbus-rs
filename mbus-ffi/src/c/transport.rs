use core::ffi::c_void;

use heapless::Vec;
use mbus_core::data_unit::common::MAX_ADU_FRAME_LEN;
use mbus_core::errors::MbusError;
use mbus_core::transport::{ModbusConfig, SerialMode, Transport, TransportType};

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

pub(super) struct CTransport {
    callbacks: MbusTransportCallbacks,
    transport_type: TransportType,
}

impl CTransport {
    pub(super) fn new_tcp(callbacks: MbusTransportCallbacks) -> Self {
        Self {
            callbacks,
            transport_type: TransportType::CustomTcp,
        }
    }

    pub(super) fn new_serial(callbacks: MbusTransportCallbacks, mode: SerialMode) -> Self {
        Self {
            callbacks,
            transport_type: TransportType::CustomSerial(mode),
        }
    }
}

pub(super) fn validate_transport_callbacks(callbacks: &MbusTransportCallbacks) -> bool {
    callbacks.on_connect.is_some()
        && callbacks.on_disconnect.is_some()
        && callbacks.on_send.is_some()
        && callbacks.on_recv.is_some()
        && callbacks.on_is_connected.is_some()
}

impl Transport for CTransport {
    type Error = MbusError;
    const SUPPORTS_BROADCAST_WRITES: bool = true;

    fn connect(&mut self, _config: &ModbusConfig) -> Result<(), Self::Error> {
        let cb = self
            .callbacks
            .on_connect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(self.callbacks.userdata) };
        status_to_result(status)
    }

    fn disconnect(&mut self) -> Result<(), Self::Error> {
        let cb = self
            .callbacks
            .on_disconnect
            .ok_or(MbusError::InvalidConfiguration)?;
        let status = unsafe { cb(self.callbacks.userdata) };
        status_to_result(status)
    }

    fn send(&mut self, adu: &[u8]) -> Result<(), Self::Error> {
        let cb = self
            .callbacks
            .on_send
            .ok_or(MbusError::InvalidConfiguration)?;
        let len = u16::try_from(adu.len()).map_err(|_| MbusError::BufferTooSmall)?;
        let status = unsafe { cb(adu.as_ptr(), len, self.callbacks.userdata) };
        status_to_result(status)
    }

    fn recv(&mut self) -> Result<Vec<u8, MAX_ADU_FRAME_LEN>, Self::Error> {
        let cb = self
            .callbacks
            .on_recv
            .ok_or(MbusError::InvalidConfiguration)?;

        let mut buf = [0u8; MAX_ADU_FRAME_LEN];
        let mut out_len: u16 = 0;
        let cap = u16::try_from(MAX_ADU_FRAME_LEN).map_err(|_| MbusError::BufferTooSmall)?;

        let status = unsafe {
            cb(
                buf.as_mut_ptr(),
                cap,
                &mut out_len as *mut u16,
                self.callbacks.userdata,
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

    fn is_connected(&self) -> bool {
        match self.callbacks.on_is_connected {
            Some(cb) => unsafe { cb(self.callbacks.userdata) != 0 },
            None => false,
        }
    }

    fn transport_type(&self) -> TransportType {
        self.transport_type
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
