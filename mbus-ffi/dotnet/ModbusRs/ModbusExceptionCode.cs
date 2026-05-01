namespace ModbusRs;

/// <summary>
/// Modbus exception codes returned by a server in response to a client
/// request. Values match the Modbus specification (and the Rust
/// <c>ExceptionCode</c> enum).
/// </summary>
public enum ModbusExceptionCode
{
    /// <summary>The requested function code is not supported.</summary>
    IllegalFunction = 1,

    /// <summary>The data address is outside the valid range.</summary>
    IllegalDataAddress = 2,

    /// <summary>The data value is not acceptable.</summary>
    IllegalDataValue = 3,

    /// <summary>An unrecoverable error occurred in the server.</summary>
    ServerDeviceFailure = 4,
}
