using System;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using ModbusRs.Native;

namespace ModbusRs;

/// <summary>
/// Modbus TCP server backed by the native <c>mbus_ffi</c> cdylib.
/// Dispatches incoming requests to a user-provided
/// <see cref="ModbusRequestHandler"/>.
/// </summary>
/// <remarks>
/// <para>
/// Call <see cref="Start"/> to begin accepting connections. The server
/// runs on a background OS thread managed by the native Tokio runtime.
/// Call <see cref="Stop"/> or <see cref="Dispose"/> to shut it down.
/// </para>
/// <para>
/// Handler methods are invoked on the Tokio thread pool, so
/// implementations must be thread-safe.
/// </para>
/// </remarks>
public sealed class ModbusTcpServer : IDisposable
{
    private readonly IntPtr _nativeHandle;
    private readonly GCHandle _handlerGcHandle;
    private bool _disposed;

    /// <summary>
    /// Creates a new Modbus TCP server.
    /// </summary>
    /// <param name="host">Bind address (e.g. <c>"0.0.0.0"</c>).</param>
    /// <param name="port">TCP port (default 502).</param>
    /// <param name="unitId">Unit ID to serve (1–247).</param>
    /// <param name="handler">Request handler implementation.</param>
    /// <exception cref="ModbusException">
    /// Thrown when the native constructor returns null.
    /// </exception>
    public ModbusTcpServer(string host, ushort port, byte unitId, ModbusRequestHandler handler)
    {
        ArgumentException.ThrowIfNullOrEmpty(host);
        ArgumentNullException.ThrowIfNull(handler);

        // Keep the handler alive for the server's lifetime via a GCHandle.
        // The stable IntPtr from GCHandle.ToIntPtr is passed as `ctx`.
        _handlerGcHandle = GCHandle.Alloc(handler);
        var ctx = GCHandle.ToIntPtr(_handlerGcHandle);

        unsafe
        {
            var vtable = BuildVtable((void*)ctx);
            _nativeHandle = NativeMethods.mbus_dn_tcp_server_new(host, port, unitId, &vtable);
        }

        if (_nativeHandle == IntPtr.Zero)
        {
            _handlerGcHandle.Free();
            throw new ModbusException(ModbusStatus.InvalidConfiguration,
                $"mbus_dn_tcp_server_new('{host}', {port}, unit={unitId}) returned null");
        }
    }

    /// <summary>Starts accepting connections (non-blocking).</summary>
    public void Start()
    {
        ThrowIfDisposed();
        var status = NativeMethods.mbus_dn_tcp_server_start(_nativeHandle);
        ModbusException.ThrowIfError(status, nameof(Start));
    }

    /// <summary>Signals the server to stop and waits for it to shut down.</summary>
    public void Stop()
    {
        ThrowIfDisposed();
        NativeMethods.mbus_dn_tcp_server_stop(_nativeHandle);
    }

    // ── IDisposable ──────────────────────────────────────────────────────

    private void ThrowIfDisposed() => ObjectDisposedException.ThrowIf(_disposed, this);

    /// <inheritdoc />
    public void Dispose()
    {
        if (_disposed) return;
        _disposed = true;
        NativeMethods.mbus_dn_tcp_server_stop(_nativeHandle);
        NativeMethods.mbus_dn_tcp_server_free(_nativeHandle);
        _handlerGcHandle.Free();
    }

    // ── Vtable construction ──────────────────────────────────────────────

    private static unsafe MbusDnServerVtable BuildVtable(void* ctx)
    {
        return new MbusDnServerVtable
        {
            ctx = ctx,
            read_coils = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, byte*, ushort*, int>)&CbReadCoils,
            write_single_coil = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, byte, int>)&CbWriteSingleCoil,
            write_multiple_coils = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, byte*, ushort, ushort, int>)&CbWriteMultipleCoils,
            read_discrete_inputs = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, byte*, ushort*, int>)&CbReadDiscreteInputs,
            read_holding_registers = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, ushort*, ushort*, int>)&CbReadHoldingRegisters,
            read_input_registers = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, ushort*, ushort*, int>)&CbReadInputRegisters,
            write_single_register = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, int>)&CbWriteSingleRegister,
            write_multiple_registers = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, byte*, ushort, int>)&CbWriteMultipleRegisters,
            mask_write_register = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, ushort, int>)&CbMaskWriteRegister,
            read_write_multiple_registers = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort, ushort, byte*, ushort, ushort*, ushort*, int>)&CbReadWriteMultipleRegisters,
            read_fifo_queue = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort, ushort*, ushort*, int>)&CbReadFifoQueue,
            read_exception_status = (IntPtr)(delegate* unmanaged[Cdecl]<void*, byte*, int>)&CbReadExceptionStatus,
            diagnostics = IntPtr.Zero, // FC08 (diagnostics echo) is not exposed; Rust layer handles the echo response directly
            get_comm_event_counter = (IntPtr)(delegate* unmanaged[Cdecl]<void*, ushort*, ushort*, int>)&CbGetCommEventCounter,
            get_comm_event_log = (IntPtr)(delegate* unmanaged[Cdecl]<void*, byte*, ushort*, int>)&CbGetCommEventLog,
            report_server_id = (IntPtr)(delegate* unmanaged[Cdecl]<void*, byte*, ushort*, int>)&CbReportServerId,
        };
    }

    // ── Static unmanaged callbacks ────────────────────────────────────────

    private static unsafe ModbusRequestHandler HandlerFromCtx(void* ctx)
        => (ModbusRequestHandler)GCHandle.FromIntPtr((IntPtr)ctx).Target!;

    private static int ExceptionCodeOf(Exception ex) => ex switch
    {
        ModbusServerException mse => (int)mse.Code,
        _ => (int)ModbusExceptionCode.ServerDeviceFailure,
    };

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadCoils(void* ctx, ushort address, ushort count, byte* outBuf, ushort* outByteCount)
    {
        try
        {
            var handler = HandlerFromCtx(ctx);
            var coils = handler.ReadCoils(address, count);
            var packed = PackCoils(coils);
            int write = Math.Min(packed.Length, 256);
            for (int i = 0; i < write; i++) outBuf[i] = packed[i];
            *outByteCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbWriteSingleCoil(void* ctx, ushort address, byte value)
    {
        try { HandlerFromCtx(ctx).WriteSingleCoil(address, value != 0); return 0; }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbWriteMultipleCoils(void* ctx, ushort address, byte* packedBytes, ushort byteCount, ushort coilCount)
    {
        try
        {
            var packed = new byte[byteCount];
            for (int i = 0; i < byteCount; i++) packed[i] = packedBytes[i];
            var coils = UnpackCoils(packed, coilCount);
            HandlerFromCtx(ctx).WriteMultipleCoils(address, coils);
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadDiscreteInputs(void* ctx, ushort address, ushort count, byte* outBuf, ushort* outByteCount)
    {
        try
        {
            var handler = HandlerFromCtx(ctx);
            var inputs = handler.ReadDiscreteInputs(address, count);
            var packed = PackCoils(inputs);
            int write = Math.Min(packed.Length, 256);
            for (int i = 0; i < write; i++) outBuf[i] = packed[i];
            *outByteCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadHoldingRegisters(void* ctx, ushort address, ushort count, ushort* outBuf, ushort* outCount)
    {
        try
        {
            var regs = HandlerFromCtx(ctx).ReadHoldingRegisters(address, count);
            int write = Math.Min(regs.Length, 128);
            for (int i = 0; i < write; i++) outBuf[i] = regs[i];
            *outCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadInputRegisters(void* ctx, ushort address, ushort count, ushort* outBuf, ushort* outCount)
    {
        try
        {
            var regs = HandlerFromCtx(ctx).ReadInputRegisters(address, count);
            int write = Math.Min(regs.Length, 128);
            for (int i = 0; i < write; i++) outBuf[i] = regs[i];
            *outCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbWriteSingleRegister(void* ctx, ushort address, ushort value)
    {
        try { HandlerFromCtx(ctx).WriteSingleRegister(address, value); return 0; }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbWriteMultipleRegisters(void* ctx, ushort address, byte* valuesBeBytes, ushort count)
    {
        try
        {
            // Values arrive as big-endian byte pairs
            var regs = new ushort[count];
            for (int i = 0; i < count; i++)
            {
                regs[i] = (ushort)((valuesBeBytes[i * 2] << 8) | valuesBeBytes[i * 2 + 1]);
            }
            HandlerFromCtx(ctx).WriteMultipleRegisters(address, regs);
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbMaskWriteRegister(void* ctx, ushort address, ushort andMask, ushort orMask)
    {
        try { HandlerFromCtx(ctx).MaskWriteRegister(address, andMask, orMask); return 0; }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadWriteMultipleRegisters(
        void* ctx, ushort readAddr, ushort readCount,
        ushort writeAddr, byte* writeBeBytes, ushort writeCount,
        ushort* outBuf, ushort* outCount)
    {
        try
        {
            var writeRegs = new ushort[writeCount];
            for (int i = 0; i < writeCount; i++)
            {
                writeRegs[i] = (ushort)((writeBeBytes[i * 2] << 8) | writeBeBytes[i * 2 + 1]);
            }
            var result = HandlerFromCtx(ctx).ReadWriteMultipleRegisters(readAddr, readCount, writeAddr, writeRegs);
            int write = Math.Min(result.Length, 128);
            for (int i = 0; i < write; i++) outBuf[i] = result[i];
            *outCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadFifoQueue(void* ctx, ushort address, ushort* outBuf, ushort* outCount)
    {
        try
        {
            var result = HandlerFromCtx(ctx).ReadFifoQueue(address);
            int write = Math.Min(result.Length, 128);
            for (int i = 0; i < write; i++) outBuf[i] = result[i];
            *outCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReadExceptionStatus(void* ctx, byte* outStatus)
    {
        try { *outStatus = HandlerFromCtx(ctx).ReadExceptionStatus(); return 0; }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbGetCommEventCounter(void* ctx, ushort* outStatus, ushort* outEventCount)
    {
        try
        {
            var (s, e) = HandlerFromCtx(ctx).GetCommEventCounter();
            *outStatus = s;
            *outEventCount = e;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbGetCommEventLog(void* ctx, byte* outBuf, ushort* outByteCount)
    {
        try
        {
            var log = HandlerFromCtx(ctx).GetCommEventLog();
            int write = Math.Min(log.Length, 256);
            for (int i = 0; i < write; i++) outBuf[i] = log[i];
            *outByteCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static unsafe int CbReportServerId(void* ctx, byte* outBuf, ushort* outByteCount)
    {
        try
        {
            var id = HandlerFromCtx(ctx).ReportServerId();
            int write = Math.Min(id.Length, 256);
            for (int i = 0; i < write; i++) outBuf[i] = id[i];
            *outByteCount = (ushort)write;
            return 0;
        }
        catch (Exception ex) { return ExceptionCodeOf(ex); }
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
