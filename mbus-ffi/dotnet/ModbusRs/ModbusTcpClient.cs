using System;
using System.Threading;
using System.Threading.Tasks;
using ModbusRs.Native;

namespace ModbusRs;

/// <summary>
/// Asynchronous Modbus TCP client backed by the native <c>mbus_ffi</c>
/// cdylib. Each request method returns a <see cref="Task{TResult}"/>
/// which completes when the underlying Tokio runtime finishes the
/// request.
/// </summary>
/// <remarks>
/// <para>
/// <b>Threading.</b> All native request entry points block the calling
/// thread on the shared Tokio runtime. To preserve the <c>Task</c>-based
/// async surface this wrapper offloads them to the .NET thread pool via
/// <see cref="Task.Run(Action)"/>. A future native upgrade can swap in a
/// completion-callback implementation without changing this surface.
/// </para>
/// <para>
/// <b>Lifetime.</b> The instance owns a <see cref="SafeHandle"/> and
/// implements <see cref="IDisposable"/>. <see cref="Dispose"/> tears down
/// the underlying Rust background task and releases the FFI handle. The
/// finalizer reachable through the SafeHandle ensures cleanup even if
/// <c>Dispose</c> is missed.
/// </para>
/// </remarks>
public sealed class ModbusTcpClient : IDisposable
{
    private readonly SafeTcpClientHandle _handle;
    private bool _disposed;

    /// <summary>
    /// Creates a new TCP client targeting <paramref name="host"/>
    /// (NUL-terminated UTF-8) and <paramref name="port"/>.
    /// </summary>
    /// <remarks>
    /// The constructor only sets up local state — call
    /// <see cref="ConnectAsync"/> before issuing requests.
    /// </remarks>
    /// <exception cref="ModbusException">
    /// Thrown when the native constructor returns null (invalid host or
    /// other configuration failure).
    /// </exception>
    public ModbusTcpClient(string host, ushort port = 502)
    {
        ArgumentException.ThrowIfNullOrEmpty(host);
        _handle = SafeTcpClientHandle.Create(host, port);
    }

    /// <summary>
    /// Returns <c>true</c> if there are requests in flight awaiting a
    /// response.
    /// </summary>
    public bool HasPendingRequests
    {
        get
        {
            ThrowIfDisposed();
            return NativeMethods.mbus_dn_tcp_client_has_pending_requests(_handle.DangerousHandle) != 0;
        }
    }

    /// <summary>
    /// Sets a per-request timeout. Pass <see cref="TimeSpan.Zero"/> to
    /// disable. Takes effect on the next request.
    /// </summary>
    public void SetRequestTimeout(TimeSpan timeout)
    {
        ThrowIfDisposed();
        ulong ms = timeout <= TimeSpan.Zero ? 0UL : (ulong)timeout.TotalMilliseconds;
        var status = NativeMethods.mbus_dn_tcp_client_set_request_timeout_ms(
            _handle.DangerousHandle, ms);
        ModbusException.ThrowIfError(status, nameof(SetRequestTimeout));
    }

    /// <summary>
    /// Establishes the underlying TCP connection.
    /// </summary>
    public Task ConnectAsync(CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var status = NativeMethods.mbus_dn_tcp_client_connect(_handle.DangerousHandle);
            ModbusException.ThrowIfError(status, nameof(ConnectAsync));
        }, cancellationToken);
    }

    /// <summary>
    /// Closes the TCP transport gracefully. The client can be reconnected
    /// with <see cref="ConnectAsync"/>.
    /// </summary>
    public Task DisconnectAsync(CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var status = NativeMethods.mbus_dn_tcp_client_disconnect(_handle.DangerousHandle);
            ModbusException.ThrowIfError(status, nameof(DisconnectAsync));
        }, cancellationToken);
    }

    // ── FC03 ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Reads <paramref name="quantity"/> holding registers (FC03) starting
    /// at <paramref name="address"/> from the given <paramref name="unitId"/>.
    /// </summary>
    public Task<ushort[]> ReadHoldingRegistersAsync(
        byte unitId,
        ushort address,
        ushort quantity,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        if (quantity == 0)
        {
            return Task.FromResult(Array.Empty<ushort>());
        }

        return Task.Run(() =>
        {
            var buf = new ushort[quantity];
            ushort actual = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* bufPtr = buf)
                {
                    status = NativeMethods.mbus_dn_tcp_client_read_holding_registers(
                        _handle.DangerousHandle,
                        unitId,
                        address,
                        quantity,
                        bufPtr,
                        quantity,
                        &actual);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadHoldingRegistersAsync));
            if (actual != quantity)
            {
                Array.Resize(ref buf, actual);
            }
            return buf;
        }, cancellationToken);
    }

    // ── FC06 ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Writes a single holding register (FC06).
    /// </summary>
    /// <returns>The echoed <c>(address, value)</c> pair from the server.</returns>
    public Task<(ushort Address, ushort Value)> WriteSingleRegisterAsync(
        byte unitId,
        ushort address,
        ushort value,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            ushort echoAddr = 0;
            ushort echoVal = 0;
            ModbusStatus status;
            unsafe
            {
                status = NativeMethods.mbus_dn_tcp_client_write_single_register(
                    _handle.DangerousHandle,
                    unitId,
                    address,
                    value,
                    &echoAddr,
                    &echoVal);
            }
            ModbusException.ThrowIfError(status, nameof(WriteSingleRegisterAsync));
            return (echoAddr, echoVal);
        }, cancellationToken);
    }

    // ── FC16 ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Writes multiple holding registers (FC16) starting at
    /// <paramref name="address"/>.
    /// </summary>
    /// <returns>The echoed <c>(starting_address, quantity)</c> pair.</returns>
    public Task<(ushort StartingAddress, ushort Quantity)> WriteMultipleRegistersAsync(
        byte unitId,
        ushort address,
        ReadOnlyMemory<ushort> values,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        if (values.Length == 0)
        {
            throw new ArgumentException("values must contain at least one register", nameof(values));
        }
        if (values.Length > ushort.MaxValue)
        {
            throw new ArgumentException("values exceeds 65535 registers", nameof(values));
        }

        // Snapshot to a stable array so the Task.Run continuation has
        // exclusive access; the caller's Memory<T> may mutate after this
        // method returns.
        var snapshot = values.ToArray();

        return Task.Run(() =>
        {
            ushort echoAddr = 0;
            ushort echoQty = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* p = snapshot)
                {
                    status = NativeMethods.mbus_dn_tcp_client_write_multiple_registers(
                        _handle.DangerousHandle,
                        unitId,
                        address,
                        p,
                        (ushort)snapshot.Length,
                        &echoAddr,
                        &echoQty);
                }
            }
            ModbusException.ThrowIfError(status, nameof(WriteMultipleRegistersAsync));
            return (echoAddr, echoQty);
        }, cancellationToken);
    }

    // ── IDisposable ──────────────────────────────────────────────────────

    private void ThrowIfDisposed()
    {
        ObjectDisposedException.ThrowIf(_disposed, this);
    }

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed)
        {
            return;
        }
        _disposed = true;
        _handle.Dispose();
    }
}
