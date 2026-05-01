using System;
using System.Runtime.InteropServices;

namespace ModbusRs.Native;

/// <summary>
/// <see cref="SafeHandle"/> wrapping the opaque <c>MbusDnSerialClient*</c>
/// returned by <c>mbus_dn_serial_client_new_rtu</c> or
/// <c>mbus_dn_serial_client_new_ascii</c>. Guarantees the native destructor
/// runs even if a managed <c>Dispose</c> is missed.
/// </summary>
internal sealed class SafeSerialClientHandle : SafeHandle
{
    private SafeSerialClientHandle() : base(IntPtr.Zero, ownsHandle: true)
    {
    }

    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <summary>Opens a new native RTU serial client.</summary>
    internal static SafeSerialClientHandle CreateRtu(
        string port, uint baudRate, byte dataBits, byte parity, byte stopBits,
        uint responseTimeoutMs)
    {
        IntPtr raw = NativeMethods.mbus_dn_serial_client_new_rtu(
            port, baudRate, dataBits, parity, stopBits, responseTimeoutMs);
        if (raw == IntPtr.Zero)
        {
            throw new ModbusException(ModbusStatus.InvalidConfiguration,
                $"mbus_dn_serial_client_new_rtu('{port}') returned null");
        }
        var safe = new SafeSerialClientHandle();
        safe.SetHandle(raw);
        return safe;
    }

    /// <summary>Opens a new native ASCII serial client.</summary>
    internal static SafeSerialClientHandle CreateAscii(
        string port, uint baudRate, byte dataBits, byte parity, byte stopBits,
        uint responseTimeoutMs)
    {
        IntPtr raw = NativeMethods.mbus_dn_serial_client_new_ascii(
            port, baudRate, dataBits, parity, stopBits, responseTimeoutMs);
        if (raw == IntPtr.Zero)
        {
            throw new ModbusException(ModbusStatus.InvalidConfiguration,
                $"mbus_dn_serial_client_new_ascii('{port}') returned null");
        }
        var safe = new SafeSerialClientHandle();
        safe.SetHandle(raw);
        return safe;
    }

    /// <summary>Returns the raw pointer for use with <see cref="NativeMethods"/>.</summary>
    internal IntPtr DangerousHandle => handle;

    protected override bool ReleaseHandle()
    {
        NativeMethods.mbus_dn_serial_client_free(handle);
        return true;
    }
}
