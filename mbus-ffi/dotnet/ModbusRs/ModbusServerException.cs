using System;

namespace ModbusRs;

/// <summary>
/// Thrown by <see cref="ModbusRequestHandler"/> methods to signal that the
/// server should respond with a Modbus exception PDU.
/// </summary>
/// <remarks>
/// Throw this from a handler override to return a specific Modbus exception
/// code to the client. Any other unhandled exception maps to
/// <see cref="ModbusExceptionCode.ServerDeviceFailure"/>.
/// </remarks>
public sealed class ModbusServerException : Exception
{
    /// <summary>The Modbus exception code to include in the response PDU.</summary>
    public ModbusExceptionCode Code { get; }

    /// <inheritdoc cref="ModbusServerException(ModbusExceptionCode, string?)"/>
    public ModbusServerException(ModbusExceptionCode code)
        : this(code, null)
    {
    }

    /// <summary>
    /// Initialises a new instance with the specified <paramref name="code"/> and
    /// an optional <paramref name="message"/>.
    /// </summary>
    public ModbusServerException(ModbusExceptionCode code, string? message)
        : base(message ?? $"Modbus server exception: {code}")
    {
        Code = code;
    }
}
