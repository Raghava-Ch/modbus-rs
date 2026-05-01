using System;
using System.Threading;
using System.Threading.Tasks;
using ModbusRs.Native;

namespace ModbusRs;

/// <summary>
/// Parity option for serial port configuration.
/// </summary>
public enum SerialParity : byte
{
    None = 0,
    Even = 1,
    Odd = 2,
}

/// <summary>
/// Asynchronous Modbus serial client backed by the native <c>mbus_ffi</c>
/// cdylib. Supports both RTU and ASCII framing.
/// </summary>
/// <remarks>
/// <para>
/// Call <see cref="ConnectAsync"/> before issuing requests. The underlying
/// serial port is opened/closed by the native runtime.
/// </para>
/// <para>
/// All native request entry points block the calling thread on the shared
/// Tokio runtime. To preserve the <c>Task</c>-based async surface this
/// wrapper offloads them to the .NET thread pool via
/// <see cref="Task.Run(Action)"/>.
/// </para>
/// </remarks>
public sealed class ModbusSerialClient : IDisposable
{
    private readonly SafeSerialClientHandle _handle;
    private bool _disposed;

    private ModbusSerialClient(SafeSerialClientHandle handle)
    {
        _handle = handle;
    }

    /// <summary>
    /// Creates a Modbus RTU serial client.
    /// </summary>
    /// <param name="port">Serial port name (e.g. <c>/dev/ttyS0</c>, <c>COM1</c>).</param>
    /// <param name="baudRate">Baud rate (e.g. 9600, 115200).</param>
    /// <param name="dataBits">Data bits (typically 8).</param>
    /// <param name="parity">Parity mode.</param>
    /// <param name="stopBits">Stop bits (typically 1 or 2).</param>
    /// <param name="responseTimeoutMs">Per-request response timeout in milliseconds.</param>
    public static ModbusSerialClient CreateRtu(
        string port,
        uint baudRate = 9600,
        byte dataBits = 8,
        SerialParity parity = SerialParity.None,
        byte stopBits = 1,
        uint responseTimeoutMs = 1000)
    {
        ArgumentException.ThrowIfNullOrEmpty(port);
        var handle = SafeSerialClientHandle.CreateRtu(port, baudRate, dataBits, (byte)parity, stopBits, responseTimeoutMs);
        return new ModbusSerialClient(handle);
    }

    /// <summary>
    /// Creates a Modbus ASCII serial client.
    /// </summary>
    /// <param name="port">Serial port name (e.g. <c>/dev/ttyS0</c>, <c>COM1</c>).</param>
    /// <param name="baudRate">Baud rate.</param>
    /// <param name="dataBits">Data bits (typically 7 or 8 for ASCII).</param>
    /// <param name="parity">Parity mode.</param>
    /// <param name="stopBits">Stop bits.</param>
    /// <param name="responseTimeoutMs">Per-request response timeout in milliseconds.</param>
    public static ModbusSerialClient CreateAscii(
        string port,
        uint baudRate = 9600,
        byte dataBits = 7,
        SerialParity parity = SerialParity.Even,
        byte stopBits = 1,
        uint responseTimeoutMs = 1000)
    {
        ArgumentException.ThrowIfNullOrEmpty(port);
        var handle = SafeSerialClientHandle.CreateAscii(port, baudRate, dataBits, (byte)parity, stopBits, responseTimeoutMs);
        return new ModbusSerialClient(handle);
    }

    /// <summary>Sets the per-request timeout. Pass <see cref="TimeSpan.Zero"/> to disable.</summary>
    public void SetRequestTimeout(TimeSpan timeout)
    {
        ThrowIfDisposed();
        ulong ms = timeout <= TimeSpan.Zero ? 0UL : (ulong)timeout.TotalMilliseconds;
        NativeMethods.mbus_dn_serial_client_set_request_timeout_ms(_handle.DangerousHandle, ms);
    }

    /// <summary>Opens the serial port and connects the Modbus transport.</summary>
    public Task ConnectAsync(CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var status = NativeMethods.mbus_dn_serial_client_connect(_handle.DangerousHandle);
            ModbusException.ThrowIfError(status, nameof(ConnectAsync));
        }, cancellationToken);
    }

    /// <summary>Closes the serial port transport.</summary>
    public Task DisconnectAsync(CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var status = NativeMethods.mbus_dn_serial_client_disconnect(_handle.DangerousHandle);
            ModbusException.ThrowIfError(status, nameof(DisconnectAsync));
        }, cancellationToken);
    }

    // ── FC01 ─────────────────────────────────────────────────────────────

    /// <summary>Reads coils (FC01).</summary>
    public Task<bool[]> ReadCoilsAsync(
        byte unitId, ushort address, ushort quantity,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new byte[(quantity + 7) / 8 + 1];
            ushort count = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (byte* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_coils(
                        _handle.DangerousHandle, unitId, address, quantity,
                        p, (ushort)buf.Length, &count);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadCoilsAsync));
            return UnpackCoils(buf, quantity);
        }, cancellationToken);
    }

    // ── FC05 ─────────────────────────────────────────────────────────────

    /// <summary>Writes a single coil (FC05).</summary>
    public Task<(ushort Address, bool Value)> WriteSingleCoilAsync(
        byte unitId, ushort address, bool value,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            ushort echoAddr = 0;
            byte echoVal = 0;
            ModbusStatus status;
            unsafe
            {
                status = NativeMethods.mbus_dn_serial_client_write_single_coil(
                    _handle.DangerousHandle, unitId, address, value ? (byte)1 : (byte)0,
                    &echoAddr, &echoVal);
            }
            ModbusException.ThrowIfError(status, nameof(WriteSingleCoilAsync));
            return (echoAddr, echoVal != 0);
        }, cancellationToken);
    }

    // ── FC0F ─────────────────────────────────────────────────────────────

    /// <summary>Writes multiple coils (FC0F).</summary>
    public Task<(ushort StartingAddress, ushort Quantity)> WriteMultipleCoilsAsync(
        byte unitId, ushort address, bool[] values,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        ArgumentNullException.ThrowIfNull(values);
        var packed = PackCoils(values);
        return Task.Run(() =>
        {
            ushort echoAddr = 0;
            ushort echoQty = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (byte* p = packed)
                {
                    status = NativeMethods.mbus_dn_serial_client_write_multiple_coils(
                        _handle.DangerousHandle, unitId, address,
                        p, (ushort)packed.Length, (ushort)values.Length,
                        &echoAddr, &echoQty);
                }
            }
            ModbusException.ThrowIfError(status, nameof(WriteMultipleCoilsAsync));
            return (echoAddr, echoQty);
        }, cancellationToken);
    }

    // ── FC02 ─────────────────────────────────────────────────────────────

    /// <summary>Reads discrete inputs (FC02).</summary>
    public Task<bool[]> ReadDiscreteInputsAsync(
        byte unitId, ushort address, ushort quantity,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new byte[(quantity + 7) / 8 + 1];
            ushort count = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (byte* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_discrete_inputs(
                        _handle.DangerousHandle, unitId, address, quantity,
                        p, (ushort)buf.Length, &count);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadDiscreteInputsAsync));
            return UnpackCoils(buf, quantity);
        }, cancellationToken);
    }

    // ── FC03 ─────────────────────────────────────────────────────────────

    /// <summary>Reads holding registers (FC03).</summary>
    public Task<ushort[]> ReadHoldingRegistersAsync(
        byte unitId, ushort address, ushort quantity,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new ushort[quantity];
            ushort actual = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_holding_registers(
                        _handle.DangerousHandle, unitId, address, quantity,
                        p, quantity, &actual);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadHoldingRegistersAsync));
            if (actual != quantity) Array.Resize(ref buf, actual);
            return buf;
        }, cancellationToken);
    }

    // ── FC06 ─────────────────────────────────────────────────────────────

    /// <summary>Writes a single holding register (FC06).</summary>
    public Task<(ushort Address, ushort Value)> WriteSingleRegisterAsync(
        byte unitId, ushort address, ushort value,
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
                status = NativeMethods.mbus_dn_serial_client_write_single_register(
                    _handle.DangerousHandle, unitId, address, value,
                    &echoAddr, &echoVal);
            }
            ModbusException.ThrowIfError(status, nameof(WriteSingleRegisterAsync));
            return (echoAddr, echoVal);
        }, cancellationToken);
    }

    // ── FC16 ─────────────────────────────────────────────────────────────

    /// <summary>Writes multiple holding registers (FC16).</summary>
    public Task<(ushort StartingAddress, ushort Quantity)> WriteMultipleRegistersAsync(
        byte unitId, ushort address, ReadOnlyMemory<ushort> values,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        if (values.Length == 0) throw new ArgumentException("values must not be empty", nameof(values));
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
                    status = NativeMethods.mbus_dn_serial_client_write_multiple_registers(
                        _handle.DangerousHandle, unitId, address,
                        p, (ushort)snapshot.Length,
                        &echoAddr, &echoQty);
                }
            }
            ModbusException.ThrowIfError(status, nameof(WriteMultipleRegistersAsync));
            return (echoAddr, echoQty);
        }, cancellationToken);
    }

    // ── FC04 ─────────────────────────────────────────────────────────────

    /// <summary>Reads input registers (FC04).</summary>
    public Task<ushort[]> ReadInputRegistersAsync(
        byte unitId, ushort address, ushort quantity,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new ushort[quantity];
            ushort actual = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_input_registers(
                        _handle.DangerousHandle, unitId, address, quantity,
                        p, quantity, &actual);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadInputRegistersAsync));
            if (actual != quantity) Array.Resize(ref buf, actual);
            return buf;
        }, cancellationToken);
    }

    // ── FC22 ─────────────────────────────────────────────────────────────

    /// <summary>Mask-writes a single register (FC22).</summary>
    public Task MaskWriteRegisterAsync(
        byte unitId, ushort address, ushort andMask, ushort orMask,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var status = NativeMethods.mbus_dn_serial_client_mask_write_register(
                _handle.DangerousHandle, unitId, address, andMask, orMask);
            ModbusException.ThrowIfError(status, nameof(MaskWriteRegisterAsync));
        }, cancellationToken);
    }

    // ── FC23 ─────────────────────────────────────────────────────────────

    /// <summary>Performs a combined read-write of multiple registers (FC23).</summary>
    public Task<ushort[]> ReadWriteMultipleRegistersAsync(
        byte unitId,
        ushort readAddress, ushort readQuantity,
        ushort writeAddress, ReadOnlyMemory<ushort> writeValues,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        var writeSnapshot = writeValues.ToArray();
        return Task.Run(() =>
        {
            var buf = new ushort[readQuantity];
            ushort actual = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* rp = buf)
                fixed (ushort* wp = writeSnapshot)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_write_multiple_registers(
                        _handle.DangerousHandle, unitId,
                        readAddress, readQuantity,
                        writeAddress, wp, (ushort)writeSnapshot.Length,
                        rp, readQuantity, &actual);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadWriteMultipleRegistersAsync));
            if (actual != readQuantity) Array.Resize(ref buf, actual);
            return buf;
        }, cancellationToken);
    }

    // ── FC24 ─────────────────────────────────────────────────────────────

    /// <summary>Reads a FIFO queue (FC24).</summary>
    public Task<ushort[]> ReadFifoQueueAsync(
        byte unitId, ushort address,
        CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new ushort[32];
            ushort actual = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (ushort* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_read_fifo_queue(
                        _handle.DangerousHandle, unitId, address,
                        p, (ushort)buf.Length, &actual);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReadFifoQueueAsync));
            if (actual != buf.Length) Array.Resize(ref buf, actual);
            return buf;
        }, cancellationToken);
    }

    // ── FC07 ─────────────────────────────────────────────────────────────

    /// <summary>Reads the exception status byte (FC07).</summary>
    public Task<byte> ReadExceptionStatusAsync(
        byte unitId, CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            byte result = 0;
            ModbusStatus status;
            unsafe
            {
                status = NativeMethods.mbus_dn_serial_client_read_exception_status(
                    _handle.DangerousHandle, unitId, &result);
            }
            ModbusException.ThrowIfError(status, nameof(ReadExceptionStatusAsync));
            return result;
        }, cancellationToken);
    }

    // ── FC0B ─────────────────────────────────────────────────────────────

    /// <summary>Reads the communication event counter (FC0B).</summary>
    public Task<(ushort Status, ushort EventCount)> GetCommEventCounterAsync(
        byte unitId, CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            ushort statusWord = 0;
            ushort eventCount = 0;
            ModbusStatus mbStatus;
            unsafe
            {
                mbStatus = NativeMethods.mbus_dn_serial_client_get_comm_event_counter(
                    _handle.DangerousHandle, unitId, &statusWord, &eventCount);
            }
            ModbusException.ThrowIfError(mbStatus, nameof(GetCommEventCounterAsync));
            return (statusWord, eventCount);
        }, cancellationToken);
    }

    // ── FC0C ─────────────────────────────────────────────────────────────

    /// <summary>Reads the communication event log (FC0C).</summary>
    public Task<byte[]> GetCommEventLogAsync(
        byte unitId, CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new byte[256];
            ushort count = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (byte* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_get_comm_event_log(
                        _handle.DangerousHandle, unitId,
                        p, (ushort)buf.Length, &count);
                }
            }
            ModbusException.ThrowIfError(status, nameof(GetCommEventLogAsync));
            if (count != buf.Length) Array.Resize(ref buf, count);
            return buf;
        }, cancellationToken);
    }

    // ── FC11 ─────────────────────────────────────────────────────────────

    /// <summary>Retrieves the server identification bytes (FC11).</summary>
    public Task<byte[]> ReportServerIdAsync(
        byte unitId, CancellationToken cancellationToken = default)
    {
        ThrowIfDisposed();
        return Task.Run(() =>
        {
            var buf = new byte[256];
            ushort count = 0;
            ModbusStatus status;
            unsafe
            {
                fixed (byte* p = buf)
                {
                    status = NativeMethods.mbus_dn_serial_client_report_server_id(
                        _handle.DangerousHandle, unitId,
                        p, (ushort)buf.Length, &count);
                }
            }
            ModbusException.ThrowIfError(status, nameof(ReportServerIdAsync));
            if (count != buf.Length) Array.Resize(ref buf, count);
            return buf;
        }, cancellationToken);
    }

    // ── IDisposable ──────────────────────────────────────────────────────

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        _handle.Dispose();
    }

    // ── Coil packing helpers ─────────────────────────────────────────────

    private static byte[] PackCoils(bool[] coils)
    {
        int byteCount = (coils.Length + 7) / 8;
        var packed = new byte[byteCount];
        for (int i = 0; i < coils.Length; i++)
        {
            if (coils[i]) packed[i / 8] |= (byte)(1 << (i % 8));
        }
        return packed;
    }

    private static bool[] UnpackCoils(byte[] packed, ushort count)
    {
        var result = new bool[count];
        for (int i = 0; i < count; i++)
        {
            result[i] = (packed[i / 8] & (1 << (i % 8))) != 0;
        }
        return result;
    }
}
