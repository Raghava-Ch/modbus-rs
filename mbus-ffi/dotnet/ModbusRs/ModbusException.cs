using System;

namespace ModbusRs;

/// <summary>
/// Thrown when a native <c>mbus_dn_*</c> entry point returns a non-OK
/// <see cref="ModbusStatus"/>. The <see cref="Status"/> property exposes
/// the underlying numeric code.
/// </summary>
public sealed class ModbusException : Exception
{
    /// <summary>The native status returned by the failing call.</summary>
    public ModbusStatus Status { get; }

    internal ModbusException(ModbusStatus status, string message)
        : base($"{message}: {status} ({(int)status})")
    {
        Status = status;
    }

    /// <summary>
    /// Throws a <see cref="ModbusException"/> if <paramref name="status"/>
    /// is not <see cref="ModbusStatus.Ok"/>.
    /// </summary>
    internal static void ThrowIfError(ModbusStatus status, string operation)
    {
        if (status != ModbusStatus.Ok)
        {
            throw new ModbusException(status, operation);
        }
    }
}
