namespace ModbusRs;

/// <summary>
/// Status code returned by every native <c>mbus_dn_*</c> entry point.
/// Numerically identical to the C-binding's <c>MbusStatusCode</c>; see
/// <c>target/mbus-ffi/include/modbus_rs_dotnet.h</c> for the canonical
/// definition.
/// </summary>
/// <remarks>
/// New variants must be appended in the same order they are added in
/// <c>mbus-ffi/src/c/error.rs</c>. Discriminants must remain stable: the
/// underlying enum is <c>#[repr(C)]</c> with implicit ordering.
/// </remarks>
public enum ModbusStatus
{
    Ok = 0,
    ParseError,
    BasicParseError,
    Timeout,
    ModbusException,
    IoError,
    Unexpected,
    ConnectionLost,
    UnsupportedFunction,
    ReservedSubFunction,
    InvalidPduLength,
    InvalidAduLength,
    ConnectionFailed,
    ConnectionClosed,
    BufferTooSmall,
    BufferLenMismatch,
    SendFailed,
    InvalidAddress,
    InvalidOffset,
    TooManyRequests,
    InvalidFunctionCode,
    NoRetriesLeft,
    TooManyFileReadSubRequests,
    FileReadPduOverflow,
    UnexpectedResponse,
    InvalidTransport,
    InvalidSlaveAddress,
    ChecksumError,
    InvalidConfiguration,
    InvalidNumOfExpectedRsps,
    InvalidDataLen,
    InvalidQuantity,
    InvalidValue,
    InvalidAndMask,
    InvalidOrMask,
    InvalidByteCount,
    InvalidDeviceIdentification,
    InvalidDeviceIdCode,
    InvalidMeiType,
    InvalidBroadcastAddress,
    BroadcastNotAllowed,
    NullPointer,
    InvalidUtf8,
    InvalidClientId,
    PoolFull,
    ClientTypeMismatch,
    Busy,
}
