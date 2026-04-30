use core::ffi::c_char;
use mbus_core::errors::MbusError;

/// C-compatible status code returned by every `mbus_*` function.
///
/// `MBUS_OK` (0) means the request was successfully queued. Actual response data
/// is delivered later via the callbacks in `MbusCallbacks`.
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MbusStatusCode {
    /// Success.
    MbusOk = 0,
    /// Frame parse error.
    MbusErrParseError,
    /// Basic parse error.
    MbusErrBasicParseError,
    /// Response timeout.
    MbusErrTimeout,
    /// Modbus exception response.
    MbusErrModbusException,
    /// I/O error.
    MbusErrIoError,
    /// Unexpected error.
    MbusErrUnexpected,
    /// Connection lost.
    MbusErrConnectionLost,
    /// Unsupported function.
    MbusErrUnsupportedFunction,
    /// Reserved sub-function.
    MbusErrReservedSubFunction,
    /// Invalid PDU length.
    MbusErrInvalidPduLength,
    /// Invalid ADU length.
    MbusErrInvalidAduLength,
    /// Connection failed.
    MbusErrConnectionFailed,
    /// Connection closed.
    MbusErrConnectionClosed,
    /// Buffer too small.
    MbusErrBufferTooSmall,
    /// Buffer length mismatch.
    MbusErrBufferLenMismatch,
    /// Send failed.
    MbusErrSendFailed,
    /// Invalid address.
    MbusErrInvalidAddress,
    /// Invalid offset.
    MbusErrInvalidOffset,
    /// Too many in-flight requests.
    MbusErrTooManyRequests,
    /// Invalid function code.
    MbusErrInvalidFunctionCode,
    /// No retries left.
    MbusErrNoRetriesLeft,
    /// Too many file read sub-requests.
    MbusErrTooManyFileReadSubRequests,
    /// File read PDU overflow.
    MbusErrFileReadPduOverflow,
    /// Unexpected response.
    MbusErrUnexpectedResponse,
    /// Invalid transport.
    MbusErrInvalidTransport,
    /// Invalid slave address.
    MbusErrInvalidSlaveAddress,
    /// Checksum error.
    MbusErrChecksumError,
    /// Invalid configuration.
    MbusErrInvalidConfiguration,
    /// Invalid number of expected responses.
    MbusErrInvalidNumOfExpectedRsps,
    /// Invalid data length.
    MbusErrInvalidDataLen,
    /// Invalid quantity.
    MbusErrInvalidQuantity,
    /// Invalid value.
    MbusErrInvalidValue,
    /// Invalid AND mask.
    MbusErrInvalidAndMask,
    /// Invalid OR mask.
    MbusErrInvalidOrMask,
    /// Invalid byte count.
    MbusErrInvalidByteCount,
    /// Invalid device identification.
    MbusErrInvalidDeviceIdentification,
    /// Invalid device ID code.
    MbusErrInvalidDeviceIdCode,
    /// Invalid MEI type.
    MbusErrInvalidMeiType,
    /// Invalid broadcast address.
    MbusErrInvalidBroadcastAddress,
    /// Broadcast not allowed.
    MbusErrBroadcastNotAllowed,
    /// Null pointer passed.
    MbusErrNullPointer,
    /// Invalid UTF-8 string.
    MbusErrInvalidUtf8,
    /// Invalid client ID (freed or never allocated).
    MbusErrInvalidClientId,
    /// Client pool is full — no free slots.
    MbusErrPoolFull,
    /// Client type mismatch (e.g. TCP call on serial ID).
    MbusErrClientTypeMismatch,
    /// Client is busy (indicates an illegal re-entrant call from a callback).
    MbusErrBusy,
}

impl From<MbusError> for MbusStatusCode {
    fn from(e: MbusError) -> Self {
        match e {
            MbusError::ParseError => Self::MbusErrParseError,
            MbusError::BasicParseError => Self::MbusErrBasicParseError,
            MbusError::Timeout => Self::MbusErrTimeout,
            MbusError::ModbusException(_) => Self::MbusErrModbusException,
            MbusError::IoError => Self::MbusErrIoError,
            MbusError::Unexpected => Self::MbusErrUnexpected,
            MbusError::ConnectionLost => Self::MbusErrConnectionLost,
            MbusError::UnsupportedFunction(_) => Self::MbusErrUnsupportedFunction,
            MbusError::ReservedSubFunction(_) => Self::MbusErrReservedSubFunction,
            MbusError::InvalidPduLength => Self::MbusErrInvalidPduLength,
            MbusError::InvalidAduLength => Self::MbusErrInvalidAduLength,
            MbusError::ConnectionFailed => Self::MbusErrConnectionFailed,
            MbusError::ConnectionClosed => Self::MbusErrConnectionClosed,
            MbusError::BufferTooSmall => Self::MbusErrBufferTooSmall,
            MbusError::BufferLenMissmatch => Self::MbusErrBufferLenMismatch,
            MbusError::SendFailed => Self::MbusErrSendFailed,
            MbusError::InvalidAddress => Self::MbusErrInvalidAddress,
            MbusError::InvalidOffset => Self::MbusErrInvalidOffset,
            MbusError::TooManyRequests => Self::MbusErrTooManyRequests,
            MbusError::InvalidFunctionCode => Self::MbusErrInvalidFunctionCode,
            MbusError::NoRetriesLeft => Self::MbusErrNoRetriesLeft,
            MbusError::TooManyFileReadSubRequests => Self::MbusErrTooManyFileReadSubRequests,
            MbusError::FileReadPduOverflow => Self::MbusErrFileReadPduOverflow,
            MbusError::UnexpectedResponse => Self::MbusErrUnexpectedResponse,
            MbusError::InvalidTransport => Self::MbusErrInvalidTransport,
            MbusError::InvalidSlaveAddress => Self::MbusErrInvalidSlaveAddress,
            MbusError::ChecksumError => Self::MbusErrChecksumError,
            MbusError::InvalidConfiguration => Self::MbusErrInvalidConfiguration,
            MbusError::InvalidNumOfExpectedRsps => Self::MbusErrInvalidNumOfExpectedRsps,
            MbusError::InvalidDataLen => Self::MbusErrInvalidDataLen,
            MbusError::InvalidQuantity => Self::MbusErrInvalidQuantity,
            MbusError::InvalidValue => Self::MbusErrInvalidValue,
            MbusError::InvalidAndMask => Self::MbusErrInvalidAndMask,
            MbusError::InvalidOrMask => Self::MbusErrInvalidOrMask,
            MbusError::InvalidByteCount => Self::MbusErrInvalidByteCount,
            MbusError::InvalidDeviceIdentification => Self::MbusErrInvalidDeviceIdentification,
            MbusError::InvalidDeviceIdCode => Self::MbusErrInvalidDeviceIdCode,
            MbusError::InvalidMeiType => Self::MbusErrInvalidMeiType,
            MbusError::InvalidBroadcastAddress => Self::MbusErrInvalidBroadcastAddress,
            MbusError::BroadcastNotAllowed => Self::MbusErrBroadcastNotAllowed,
        }
    }
}

/// Returns a static null-terminated C string describing the status code.
///
/// The returned pointer is always valid (points to a static string literal).
/// The caller must NOT free it.
#[unsafe(no_mangle)]
pub extern "C" fn mbus_status_str(code: MbusStatusCode) -> *const c_char {
    let s: &'static [u8] = match code {
        MbusStatusCode::MbusOk => b"OK\0",
        MbusStatusCode::MbusErrParseError => b"parse error\0",
        MbusStatusCode::MbusErrBasicParseError => b"basic parse error\0",
        MbusStatusCode::MbusErrTimeout => b"timeout\0",
        MbusStatusCode::MbusErrModbusException => b"modbus exception\0",
        MbusStatusCode::MbusErrIoError => b"I/O error\0",
        MbusStatusCode::MbusErrUnexpected => b"unexpected error\0",
        MbusStatusCode::MbusErrConnectionLost => b"connection lost\0",
        MbusStatusCode::MbusErrUnsupportedFunction => b"unsupported function\0",
        MbusStatusCode::MbusErrReservedSubFunction => b"reserved sub-function\0",
        MbusStatusCode::MbusErrInvalidPduLength => b"invalid PDU length\0",
        MbusStatusCode::MbusErrInvalidAduLength => b"invalid ADU length\0",
        MbusStatusCode::MbusErrConnectionFailed => b"connection failed\0",
        MbusStatusCode::MbusErrConnectionClosed => b"connection closed\0",
        MbusStatusCode::MbusErrBufferTooSmall => b"buffer too small\0",
        MbusStatusCode::MbusErrBufferLenMismatch => b"buffer length mismatch\0",
        MbusStatusCode::MbusErrSendFailed => b"send failed\0",
        MbusStatusCode::MbusErrInvalidAddress => b"invalid address\0",
        MbusStatusCode::MbusErrInvalidOffset => b"invalid offset\0",
        MbusStatusCode::MbusErrTooManyRequests => b"too many in-flight requests\0",
        MbusStatusCode::MbusErrInvalidFunctionCode => b"invalid function code\0",
        MbusStatusCode::MbusErrNoRetriesLeft => b"no retries left\0",
        MbusStatusCode::MbusErrTooManyFileReadSubRequests => b"too many file read sub-requests\0",
        MbusStatusCode::MbusErrFileReadPduOverflow => b"file read PDU overflow\0",
        MbusStatusCode::MbusErrUnexpectedResponse => b"unexpected response\0",
        MbusStatusCode::MbusErrInvalidTransport => b"invalid transport\0",
        MbusStatusCode::MbusErrInvalidSlaveAddress => b"invalid slave address\0",
        MbusStatusCode::MbusErrChecksumError => b"checksum error\0",
        MbusStatusCode::MbusErrInvalidConfiguration => b"invalid configuration\0",
        MbusStatusCode::MbusErrInvalidNumOfExpectedRsps => {
            b"invalid number of expected responses\0"
        }
        MbusStatusCode::MbusErrInvalidDataLen => b"invalid data length\0",
        MbusStatusCode::MbusErrInvalidQuantity => b"invalid quantity\0",
        MbusStatusCode::MbusErrInvalidValue => b"invalid value\0",
        MbusStatusCode::MbusErrInvalidAndMask => b"invalid AND mask\0",
        MbusStatusCode::MbusErrInvalidOrMask => b"invalid OR mask\0",
        MbusStatusCode::MbusErrInvalidByteCount => b"invalid byte count\0",
        MbusStatusCode::MbusErrInvalidDeviceIdentification => b"invalid device identification\0",
        MbusStatusCode::MbusErrInvalidDeviceIdCode => b"invalid device ID code\0",
        MbusStatusCode::MbusErrInvalidMeiType => b"invalid MEI type\0",
        MbusStatusCode::MbusErrInvalidBroadcastAddress => b"invalid broadcast address\0",
        MbusStatusCode::MbusErrBroadcastNotAllowed => b"broadcast not allowed\0",
        MbusStatusCode::MbusErrNullPointer => b"null pointer\0",
        MbusStatusCode::MbusErrInvalidUtf8 => b"invalid UTF-8 in config string\0",
        MbusStatusCode::MbusErrInvalidClientId => b"invalid client ID\0",
        MbusStatusCode::MbusErrPoolFull => b"client pool full\0",
        MbusStatusCode::MbusErrClientTypeMismatch => b"client type mismatch\0",
        MbusStatusCode::MbusErrBusy => b"client is busy (re-entrant call)\0",
    };
    s.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::ffi::CStr;
    use mbus_core::errors::MbusError;

    // ── From<MbusError> mapping ───────────────────────────────────────────────

    #[test]
    fn from_mbus_error_maps_every_variant() {
        macro_rules! check {
            ($err:expr, $code:ident) => {
                assert_eq!(
                    MbusStatusCode::from($err),
                    MbusStatusCode::$code,
                    "wrong mapping for {:?}",
                    $err
                );
            };
        }
        check!(MbusError::ParseError, MbusErrParseError);
        check!(MbusError::BasicParseError, MbusErrBasicParseError);
        check!(MbusError::Timeout, MbusErrTimeout);
        check!(MbusError::ModbusException(1), MbusErrModbusException);
        check!(MbusError::IoError, MbusErrIoError);
        check!(MbusError::Unexpected, MbusErrUnexpected);
        check!(MbusError::ConnectionLost, MbusErrConnectionLost);
        check!(
            MbusError::UnsupportedFunction(1),
            MbusErrUnsupportedFunction
        );
        check!(
            MbusError::ReservedSubFunction(1),
            MbusErrReservedSubFunction
        );
        check!(MbusError::InvalidPduLength, MbusErrInvalidPduLength);
        check!(MbusError::InvalidAduLength, MbusErrInvalidAduLength);
        check!(MbusError::ConnectionFailed, MbusErrConnectionFailed);
        check!(MbusError::ConnectionClosed, MbusErrConnectionClosed);
        check!(MbusError::BufferTooSmall, MbusErrBufferTooSmall);
        check!(MbusError::BufferLenMissmatch, MbusErrBufferLenMismatch);
        check!(MbusError::SendFailed, MbusErrSendFailed);
        check!(MbusError::InvalidAddress, MbusErrInvalidAddress);
        check!(MbusError::InvalidOffset, MbusErrInvalidOffset);
        check!(MbusError::TooManyRequests, MbusErrTooManyRequests);
        check!(MbusError::InvalidFunctionCode, MbusErrInvalidFunctionCode);
        check!(MbusError::NoRetriesLeft, MbusErrNoRetriesLeft);
        check!(
            MbusError::TooManyFileReadSubRequests,
            MbusErrTooManyFileReadSubRequests
        );
        check!(MbusError::FileReadPduOverflow, MbusErrFileReadPduOverflow);
        check!(MbusError::UnexpectedResponse, MbusErrUnexpectedResponse);
        check!(MbusError::InvalidTransport, MbusErrInvalidTransport);
        check!(MbusError::InvalidSlaveAddress, MbusErrInvalidSlaveAddress);
        check!(MbusError::ChecksumError, MbusErrChecksumError);
        check!(MbusError::InvalidConfiguration, MbusErrInvalidConfiguration);
        check!(
            MbusError::InvalidNumOfExpectedRsps,
            MbusErrInvalidNumOfExpectedRsps
        );
        check!(MbusError::InvalidDataLen, MbusErrInvalidDataLen);
        check!(MbusError::InvalidQuantity, MbusErrInvalidQuantity);
        check!(MbusError::InvalidValue, MbusErrInvalidValue);
        check!(MbusError::InvalidAndMask, MbusErrInvalidAndMask);
        check!(MbusError::InvalidOrMask, MbusErrInvalidOrMask);
        check!(MbusError::InvalidByteCount, MbusErrInvalidByteCount);
        check!(
            MbusError::InvalidDeviceIdentification,
            MbusErrInvalidDeviceIdentification
        );
        check!(MbusError::InvalidDeviceIdCode, MbusErrInvalidDeviceIdCode);
        check!(MbusError::InvalidMeiType, MbusErrInvalidMeiType);
        check!(
            MbusError::InvalidBroadcastAddress,
            MbusErrInvalidBroadcastAddress
        );
        check!(MbusError::BroadcastNotAllowed, MbusErrBroadcastNotAllowed);
    }

    // ── mbus_status_str ───────────────────────────────────────────────────────

    #[test]
    fn ok_maps_to_zero_discriminant() {
        assert_eq!(MbusStatusCode::MbusOk as u32, 0);
    }

    #[test]
    fn status_str_is_nonnull_nonempty_and_nul_terminated_for_every_variant() {
        let all: &[MbusStatusCode] = &[
            MbusStatusCode::MbusOk,
            MbusStatusCode::MbusErrParseError,
            MbusStatusCode::MbusErrBasicParseError,
            MbusStatusCode::MbusErrTimeout,
            MbusStatusCode::MbusErrModbusException,
            MbusStatusCode::MbusErrIoError,
            MbusStatusCode::MbusErrUnexpected,
            MbusStatusCode::MbusErrConnectionLost,
            MbusStatusCode::MbusErrUnsupportedFunction,
            MbusStatusCode::MbusErrReservedSubFunction,
            MbusStatusCode::MbusErrInvalidPduLength,
            MbusStatusCode::MbusErrInvalidAduLength,
            MbusStatusCode::MbusErrConnectionFailed,
            MbusStatusCode::MbusErrConnectionClosed,
            MbusStatusCode::MbusErrBufferTooSmall,
            MbusStatusCode::MbusErrBufferLenMismatch,
            MbusStatusCode::MbusErrSendFailed,
            MbusStatusCode::MbusErrInvalidAddress,
            MbusStatusCode::MbusErrInvalidOffset,
            MbusStatusCode::MbusErrTooManyRequests,
            MbusStatusCode::MbusErrInvalidFunctionCode,
            MbusStatusCode::MbusErrNoRetriesLeft,
            MbusStatusCode::MbusErrTooManyFileReadSubRequests,
            MbusStatusCode::MbusErrFileReadPduOverflow,
            MbusStatusCode::MbusErrUnexpectedResponse,
            MbusStatusCode::MbusErrInvalidTransport,
            MbusStatusCode::MbusErrInvalidSlaveAddress,
            MbusStatusCode::MbusErrChecksumError,
            MbusStatusCode::MbusErrInvalidConfiguration,
            MbusStatusCode::MbusErrInvalidNumOfExpectedRsps,
            MbusStatusCode::MbusErrInvalidDataLen,
            MbusStatusCode::MbusErrInvalidQuantity,
            MbusStatusCode::MbusErrInvalidValue,
            MbusStatusCode::MbusErrInvalidAndMask,
            MbusStatusCode::MbusErrInvalidOrMask,
            MbusStatusCode::MbusErrInvalidByteCount,
            MbusStatusCode::MbusErrInvalidDeviceIdentification,
            MbusStatusCode::MbusErrInvalidDeviceIdCode,
            MbusStatusCode::MbusErrInvalidMeiType,
            MbusStatusCode::MbusErrInvalidBroadcastAddress,
            MbusStatusCode::MbusErrBroadcastNotAllowed,
            MbusStatusCode::MbusErrNullPointer,
            MbusStatusCode::MbusErrInvalidUtf8,
            MbusStatusCode::MbusErrInvalidClientId,
            MbusStatusCode::MbusErrPoolFull,
            MbusStatusCode::MbusErrClientTypeMismatch,
            MbusStatusCode::MbusErrBusy,
        ];
        for &code in all {
            let ptr = mbus_status_str(code);
            assert!(
                !ptr.is_null(),
                "mbus_status_str returned null for {:?}",
                code
            );
            let s = unsafe { CStr::from_ptr(ptr) }
                .to_str()
                .expect("status string is not valid UTF-8");
            assert!(
                !s.is_empty(),
                "mbus_status_str returned empty string for {:?}",
                code
            );
        }
    }

    #[test]
    fn status_str_ok_is_literally_ok() {
        let ptr = mbus_status_str(MbusStatusCode::MbusOk);
        let s = unsafe { CStr::from_ptr(ptr) }.to_str().unwrap();
        assert_eq!(s, "OK");
    }

    #[test]
    fn status_str_returns_same_pointer_on_repeated_calls() {
        // Strings are static literals — the pointer must be stable.
        let p1 = mbus_status_str(MbusStatusCode::MbusErrTimeout);
        let p2 = mbus_status_str(MbusStatusCode::MbusErrTimeout);
        assert_eq!(p1, p2);
    }
}
