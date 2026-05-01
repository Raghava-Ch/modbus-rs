using System.Runtime.InteropServices;

namespace ModbusRs.Native;

/// <summary>
/// Blittable sub-request descriptor passed to the native
/// <c>mbus_dn_tcp_client_read_file_record</c>,
/// <c>mbus_dn_tcp_client_write_file_record</c>, and their serial
/// equivalents.
/// </summary>
/// <remarks>
/// <para>
/// <b>Read</b> sub-requests: set <see cref="Data"/> to <see langword="null"/> and
/// <see cref="DataLen"/> to <c>0</c>; fill <see cref="FileNumber"/>,
/// <see cref="RecordNumber"/>, and <see cref="RecordLength"/> with the target
/// record coordinates.
/// </para>
/// <para>
/// <b>Write</b> sub-requests: pin or fix the data array and store its address in
/// <see cref="Data"/>; set <see cref="DataLen"/> to the number of words and ensure
/// <see cref="RecordLength"/> == <see cref="DataLen"/>.
/// </para>
/// </remarks>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct MbusDnSubRequest
{
    /// <summary>File number (1–65535).</summary>
    public ushort FileNumber;
    /// <summary>Starting record number within the file (0–9999).</summary>
    public ushort RecordNumber;
    /// <summary>
    /// Number of 16-bit registers.
    /// For reads: how many to read.
    /// For writes: must equal <see cref="DataLen"/>.
    /// </summary>
    public ushort RecordLength;
    /// <summary>
    /// Pointer to write data (<see langword="null"/> for reads).
    /// Valid for at least <see cref="DataLen"/> words during the native call.
    /// </summary>
    public ushort* Data;
    /// <summary>Number of valid words pointed to by <see cref="Data"/> (0 for reads).</summary>
    public ushort DataLen;
}
