using System;
using System.Runtime.InteropServices;

namespace ModbusRs.Native;

/// <summary>
/// <see cref="SafeHandle"/> wrapping the opaque <c>MbusDnTcpClient*</c>
/// returned by <c>mbus_dn_tcp_client_new</c>. Guarantees the native
/// destructor runs even if a managed <c>Dispose</c> is missed.
/// </summary>
internal sealed class SafeTcpClientHandle : SafeHandle
{
    private SafeTcpClientHandle() : base(IntPtr.Zero, ownsHandle: true)
    {
    }

    public override bool IsInvalid => handle == IntPtr.Zero;

    /// <summary>
    /// Opens a new native client. Throws <see cref="ModbusException"/> if
    /// the constructor returned a null pointer (invalid host or other
    /// configuration error).
    /// </summary>
    internal static SafeTcpClientHandle Create(string host, ushort port)
    {
        IntPtr raw = NativeMethods.mbus_dn_tcp_client_new(host, port);
        if (raw == IntPtr.Zero)
        {
            throw new ModbusException(ModbusStatus.InvalidConfiguration,
                $"mbus_dn_tcp_client_new('{host}', {port}) returned null");
        }
        var safe = new SafeTcpClientHandle();
        safe.SetHandle(raw);
        return safe;
    }

    /// <summary>
    /// Returns the raw pointer for use with <see cref="NativeMethods"/>.
    /// </summary>
    internal IntPtr DangerousHandle => handle;

    protected override bool ReleaseHandle()
    {
        NativeMethods.mbus_dn_tcp_client_free(handle);
        return true;
    }
}
