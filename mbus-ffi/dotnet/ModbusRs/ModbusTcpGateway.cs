using System;
using System.Collections.Generic;
using ModbusRs.Native;

namespace ModbusRs;

/// <summary>
/// Modbus TCP gateway backed by the native <c>mbus_ffi</c> cdylib.
/// Routes incoming client requests to upstream Modbus TCP servers based
/// on unit-ID mappings.
/// </summary>
/// <remarks>
/// <para>
/// Use the fluent <see cref="AddDownstream"/>, <see cref="AddUnitRoute"/>,
/// and <see cref="AddRangeRoute"/> methods to configure routing, then call
/// <see cref="Start"/> to begin forwarding connections.
/// </para>
/// <para>
/// Downstream connections are established at <see cref="Start"/> time.
/// </para>
/// </remarks>
public sealed class ModbusTcpGateway : IDisposable
{
    private readonly IntPtr _nativeHandle;
    private bool _disposed;

    /// <summary>
    /// Creates a new gateway that listens on <paramref name="host"/>:<paramref name="port"/>.
    /// </summary>
    /// <exception cref="ModbusException">
    /// Thrown when the native constructor returns null.
    /// </exception>
    public ModbusTcpGateway(string host, ushort port = 502)
    {
        ArgumentException.ThrowIfNullOrEmpty(host);
        _nativeHandle = NativeMethods.mbus_dn_tcp_gateway_new(host, port);
        if (_nativeHandle == IntPtr.Zero)
        {
            throw new ModbusException(ModbusStatus.InvalidConfiguration,
                $"mbus_dn_tcp_gateway_new('{host}', {port}) returned null");
        }
    }

    /// <summary>
    /// Adds a downstream (upstream server) connection target.
    /// </summary>
    /// <param name="host">Downstream server host name or IP.</param>
    /// <param name="port">Downstream server TCP port (default 502).</param>
    /// <returns>
    /// A zero-based channel index to use with <see cref="AddUnitRoute"/> and
    /// <see cref="AddRangeRoute"/>.
    /// </returns>
    /// <exception cref="ModbusException">
    /// Thrown when the native call fails (e.g. too many downstreams).
    /// </exception>
    public uint AddDownstream(string host, ushort port = 502)
    {
        ThrowIfDisposed();
        ArgumentException.ThrowIfNullOrEmpty(host);
        uint channel = NativeMethods.mbus_dn_tcp_gateway_add_downstream(_nativeHandle, host, port);
        if (channel == uint.MaxValue)
        {
            throw new ModbusException(ModbusStatus.InvalidAddress,
                $"mbus_dn_tcp_gateway_add_downstream('{host}', {port}) failed");
        }
        return channel;
    }

    /// <summary>
    /// Routes a single unit ID to an existing downstream channel.
    /// </summary>
    /// <param name="unitId">Modbus unit ID (1–247).</param>
    /// <param name="channel">Channel index returned by <see cref="AddDownstream"/>.</param>
    public ModbusTcpGateway AddUnitRoute(byte unitId, uint channel)
    {
        ThrowIfDisposed();
        var status = NativeMethods.mbus_dn_tcp_gateway_add_unit_route(_nativeHandle, unitId, channel);
        ModbusException.ThrowIfError(status, nameof(AddUnitRoute));
        return this;
    }

    /// <summary>
    /// Routes an inclusive range of unit IDs to an existing downstream channel.
    /// </summary>
    /// <param name="unitMin">Minimum unit ID of the range (inclusive).</param>
    /// <param name="unitMax">Maximum unit ID of the range (inclusive).</param>
    /// <param name="channel">Channel index returned by <see cref="AddDownstream"/>.</param>
    public ModbusTcpGateway AddRangeRoute(byte unitMin, byte unitMax, uint channel)
    {
        ThrowIfDisposed();
        var status = NativeMethods.mbus_dn_tcp_gateway_add_range_route(
            _nativeHandle, unitMin, unitMax, channel);
        ModbusException.ThrowIfError(status, nameof(AddRangeRoute));
        return this;
    }

    /// <summary>Starts the gateway, connecting to all registered downstreams.</summary>
    public void Start()
    {
        ThrowIfDisposed();
        var status = NativeMethods.mbus_dn_tcp_gateway_start(_nativeHandle);
        ModbusException.ThrowIfError(status, nameof(Start));
    }

    /// <summary>Signals the gateway to stop and waits for shutdown.</summary>
    public void Stop()
    {
        ThrowIfDisposed();
        NativeMethods.mbus_dn_tcp_gateway_stop(_nativeHandle);
    }

    // ── IDisposable ──────────────────────────────────────────────────────

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        NativeMethods.mbus_dn_tcp_gateway_stop(_nativeHandle);
        NativeMethods.mbus_dn_tcp_gateway_free(_nativeHandle);
    }
}
