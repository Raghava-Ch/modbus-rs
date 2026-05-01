using System;
using System.Runtime.InteropServices;

namespace ModbusRs.Native;

/// <summary>
/// P/Invoke declarations for the <c>mbus_ffi</c> cdylib's .NET-facing
/// <c>mbus_dn_*</c> entry points.
/// </summary>
/// <remarks>
/// <para>
/// The native library is loaded by name (<c>mbus_ffi</c>); the runtime
/// resolves it on each platform's standard search path:
/// </para>
/// <list type="bullet">
///   <item>Linux: <c>libmbus_ffi.so</c></item>
///   <item>macOS: <c>libmbus_ffi.dylib</c></item>
///   <item>Windows: <c>mbus_ffi.dll</c></item>
/// </list>
/// <para>
/// All entry points use the platform default calling convention (cdecl on
/// Unix, stdcall on Windows for non-vararg fns — both of which match the
/// Rust <c>extern "C"</c> ABI).
/// </para>
/// </remarks>
internal static partial class NativeMethods
{
    public const string LibraryName = "mbus_ffi";

    // ── TCP client — lifecycle ────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_new",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr mbus_dn_tcp_client_new(string host, ushort port);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_free")]
    internal static partial void mbus_dn_tcp_client_free(IntPtr handle);

    // ── TCP client — connection ───────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_connect")]
    internal static partial ModbusStatus mbus_dn_tcp_client_connect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_disconnect")]
    internal static partial ModbusStatus mbus_dn_tcp_client_disconnect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_set_request_timeout_ms")]
    internal static partial ModbusStatus mbus_dn_tcp_client_set_request_timeout_ms(
        IntPtr handle, ulong timeoutMs);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_has_pending_requests")]
    internal static partial byte mbus_dn_tcp_client_has_pending_requests(IntPtr handle);

    // ── TCP client — FC01/FC05/FC0F (coils) ──────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_coils")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_coils(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        byte* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_single_coil")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_single_coil(
        IntPtr handle, byte unitId, ushort address, byte value,
        ushort* outAddress, byte* outValue);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_multiple_coils")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_multiple_coils(
        IntPtr handle, byte unitId, ushort address,
        byte* packedCoils, ushort byteCount, ushort coilCount,
        ushort* outAddress, ushort* outQuantity);

    // ── TCP client — FC02 (discrete inputs) ──────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_discrete_inputs")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_discrete_inputs(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        byte* outBuf, ushort outBufLen, ushort* outCount);

    // ── TCP client — FC03/FC06/FC16 (holding registers) ─────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_holding_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_holding_registers(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_single_register")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_single_register(
        IntPtr handle, byte unitId, ushort address, ushort value,
        ushort* outAddress, ushort* outValue);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_multiple_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_multiple_registers(
        IntPtr handle, byte unitId, ushort address,
        ushort* values, ushort quantity,
        ushort* outAddress, ushort* outQuantity);

    // ── TCP client — FC04 (input registers) ──────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_input_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_input_registers(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── TCP client — FC22/FC23 (register misc) ───────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_mask_write_register")]
    internal static partial ModbusStatus mbus_dn_tcp_client_mask_write_register(
        IntPtr handle, byte unitId, ushort address, ushort andMask, ushort orMask);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_write_multiple_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_write_multiple_registers(
        IntPtr handle, byte unitId,
        ushort readAddress, ushort readQuantity,
        ushort writeAddress, ushort* writeValues, ushort writeQuantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── TCP client — FC24 (FIFO) ──────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_fifo_queue")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_fifo_queue(
        IntPtr handle, byte unitId, ushort address,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── TCP client — diagnostics ──────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_exception_status")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_exception_status(
        IntPtr handle, byte unitId, byte* outStatus);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_get_comm_event_counter")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_get_comm_event_counter(
        IntPtr handle, byte unitId, ushort* outStatus, ushort* outEventCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_get_comm_event_log")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_get_comm_event_log(
        IntPtr handle, byte unitId, byte* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_report_server_id")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_report_server_id(
        IntPtr handle, byte unitId, byte* outBuf, ushort outBufLen, ushort* outCount);

    // ── Serial client — lifecycle ─────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_new_rtu",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr mbus_dn_serial_client_new_rtu(
        string port, uint baudRate, byte dataBits, byte parity, byte stopBits,
        uint responseTimeoutMs);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_new_ascii",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr mbus_dn_serial_client_new_ascii(
        string port, uint baudRate, byte dataBits, byte parity, byte stopBits,
        uint responseTimeoutMs);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_free")]
    internal static partial void mbus_dn_serial_client_free(IntPtr handle);

    // ── Serial client — connection ────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_connect")]
    internal static partial ModbusStatus mbus_dn_serial_client_connect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_disconnect")]
    internal static partial ModbusStatus mbus_dn_serial_client_disconnect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_set_request_timeout_ms")]
    internal static partial void mbus_dn_serial_client_set_request_timeout_ms(
        IntPtr handle, ulong timeoutMs);

    // ── Serial client — FC01/FC05/FC0F (coils) ────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_coils")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_coils(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        byte* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_write_single_coil")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_write_single_coil(
        IntPtr handle, byte unitId, ushort address, byte value,
        ushort* outAddress, byte* outValue);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_write_multiple_coils")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_write_multiple_coils(
        IntPtr handle, byte unitId, ushort address,
        byte* packedCoils, ushort byteCount, ushort coilCount,
        ushort* outAddress, ushort* outQuantity);

    // ── Serial client — FC02 (discrete inputs) ────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_discrete_inputs")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_discrete_inputs(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        byte* outBuf, ushort outBufLen, ushort* outCount);

    // ── Serial client — FC03/FC06/FC16 (holding registers) ───────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_holding_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_holding_registers(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_write_single_register")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_write_single_register(
        IntPtr handle, byte unitId, ushort address, ushort value,
        ushort* outAddress, ushort* outValue);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_write_multiple_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_write_multiple_registers(
        IntPtr handle, byte unitId, ushort address,
        ushort* values, ushort quantity,
        ushort* outAddress, ushort* outQuantity);

    // ── Serial client — FC04 (input registers) ────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_input_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_input_registers(
        IntPtr handle, byte unitId, ushort address, ushort quantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── Serial client — FC22/FC23 (register misc) ─────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_mask_write_register")]
    internal static partial ModbusStatus mbus_dn_serial_client_mask_write_register(
        IntPtr handle, byte unitId, ushort address, ushort andMask, ushort orMask);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_write_multiple_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_write_multiple_registers(
        IntPtr handle, byte unitId,
        ushort readAddress, ushort readQuantity,
        ushort writeAddress, ushort* writeValues, ushort writeQuantity,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── Serial client — FC24 (FIFO) ───────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_fifo_queue")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_fifo_queue(
        IntPtr handle, byte unitId, ushort address,
        ushort* outBuf, ushort outBufLen, ushort* outCount);

    // ── Serial client — diagnostics ───────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_read_exception_status")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_read_exception_status(
        IntPtr handle, byte unitId, byte* outStatus);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_get_comm_event_counter")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_get_comm_event_counter(
        IntPtr handle, byte unitId, ushort* outStatus, ushort* outEventCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_get_comm_event_log")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_get_comm_event_log(
        IntPtr handle, byte unitId, byte* outBuf, ushort outBufLen, ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_serial_client_report_server_id")]
    internal static unsafe partial ModbusStatus mbus_dn_serial_client_report_server_id(
        IntPtr handle, byte unitId, byte* outBuf, ushort outBufLen, ushort* outCount);

    // ── Server — lifecycle ────────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_server_new",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static unsafe partial IntPtr mbus_dn_tcp_server_new(
        string host, ushort port, byte unitId,
        MbusDnServerVtable* vtable);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_server_free")]
    internal static partial void mbus_dn_tcp_server_free(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_server_start")]
    internal static partial ModbusStatus mbus_dn_tcp_server_start(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_server_stop")]
    internal static partial void mbus_dn_tcp_server_stop(IntPtr handle);

    // ── Gateway — lifecycle ───────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_new",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr mbus_dn_tcp_gateway_new(string host, ushort port);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_free")]
    internal static partial void mbus_dn_tcp_gateway_free(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_add_downstream",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial uint mbus_dn_tcp_gateway_add_downstream(
        IntPtr handle, string host, ushort port);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_add_unit_route")]
    internal static partial ModbusStatus mbus_dn_tcp_gateway_add_unit_route(
        IntPtr handle, byte unitId, uint channel);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_add_range_route")]
    internal static partial ModbusStatus mbus_dn_tcp_gateway_add_range_route(
        IntPtr handle, byte unitMin, byte unitMax, uint channel);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_start")]
    internal static partial ModbusStatus mbus_dn_tcp_gateway_start(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_gateway_stop")]
    internal static partial void mbus_dn_tcp_gateway_stop(IntPtr handle);
}
