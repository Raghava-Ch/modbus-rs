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

    // ── Lifecycle ────────────────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_new",
        StringMarshalling = StringMarshalling.Utf8)]
    internal static partial IntPtr mbus_dn_tcp_client_new(string host, ushort port);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_free")]
    internal static partial void mbus_dn_tcp_client_free(IntPtr handle);

    // ── Connection ───────────────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_connect")]
    internal static partial ModbusStatus mbus_dn_tcp_client_connect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_disconnect")]
    internal static partial ModbusStatus mbus_dn_tcp_client_disconnect(IntPtr handle);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_set_request_timeout_ms")]
    internal static partial ModbusStatus mbus_dn_tcp_client_set_request_timeout_ms(
        IntPtr handle, ulong timeoutMs);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_has_pending_requests")]
    internal static partial byte mbus_dn_tcp_client_has_pending_requests(IntPtr handle);

    // ── Request entry points ─────────────────────────────────────────────

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_read_holding_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_read_holding_registers(
        IntPtr handle,
        byte unitId,
        ushort address,
        ushort quantity,
        ushort* outBuf,
        ushort outBufLen,
        ushort* outCount);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_single_register")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_single_register(
        IntPtr handle,
        byte unitId,
        ushort address,
        ushort value,
        ushort* outAddress,
        ushort* outValue);

    [LibraryImport(LibraryName, EntryPoint = "mbus_dn_tcp_client_write_multiple_registers")]
    internal static unsafe partial ModbusStatus mbus_dn_tcp_client_write_multiple_registers(
        IntPtr handle,
        byte unitId,
        ushort address,
        ushort* values,
        ushort quantity,
        ushort* outAddress,
        ushort* outQuantity);
}
