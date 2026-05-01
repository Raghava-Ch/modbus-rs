using System;
using System.Runtime.InteropServices;

namespace ModbusRs.Native;

/// <summary>
/// C-compatible vtable passed to <c>mbus_dn_tcp_server_new</c>.
/// Field order and types must exactly match <c>MbusDnServerVtable</c> in
/// <c>mbus-ffi/src/dotnet/server/vtable.rs</c>.
/// </summary>
/// <remarks>
/// Every function-pointer slot is an <see cref="IntPtr"/>; a value of
/// <see cref="IntPtr.Zero"/> means "not implemented" and causes the server
/// to return an <c>IllegalFunction</c> exception for that function code.
/// <para>
/// Positive callback return values are interpreted as Modbus exception codes
/// (1 = IllegalFunction, 2 = IllegalDataAddress, 3 = IllegalDataValue,
/// 4 = ServerDeviceFailure). Negative values map to ServerDeviceFailure.
/// Zero means success.
/// </para>
/// </remarks>
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct MbusDnServerVtable
{
    /// <summary>Opaque context forwarded unchanged to every callback.</summary>
    public void* ctx;

    // FC01 — fn(ctx, address, count, out_packed_bytes, out_byte_count) -> i32
    public IntPtr read_coils;

    // FC05 — fn(ctx, address, value_bool) -> i32
    public IntPtr write_single_coil;

    // FC0F — fn(ctx, address, packed_bytes, byte_count, coil_count) -> i32
    public IntPtr write_multiple_coils;

    // FC02 — fn(ctx, address, count, out_packed_bytes, out_byte_count) -> i32
    public IntPtr read_discrete_inputs;

    // FC03 — fn(ctx, address, count, out_u16_values, out_count) -> i32
    public IntPtr read_holding_registers;

    // FC04 — fn(ctx, address, count, out_u16_values, out_count) -> i32
    public IntPtr read_input_registers;

    // FC06 — fn(ctx, address, value) -> i32
    public IntPtr write_single_register;

    // FC10 — fn(ctx, address, values_be_bytes, count) -> i32
    public IntPtr write_multiple_registers;

    // FC16 — fn(ctx, address, and_mask, or_mask) -> i32
    public IntPtr mask_write_register;

    // FC17 — fn(ctx, read_addr, read_count, write_addr, write_be_bytes, write_count, out_u16, out_count) -> i32
    public IntPtr read_write_multiple_registers;

    // FC18 — fn(ctx, pointer_address, out_u16_values, out_count) -> i32
    public IntPtr read_fifo_queue;

    // FC07 — fn(ctx, out_status_byte) -> i32
    public IntPtr read_exception_status;

    // FC08 — fn(ctx, sub_fn, data, out_sub_fn, out_data) -> i32
    public IntPtr diagnostics;

    // FC0B — fn(ctx, out_status_word, out_event_count) -> i32
    public IntPtr get_comm_event_counter;

    // FC0C — fn(ctx, out_payload_bytes, out_byte_count) -> i32
    public IntPtr get_comm_event_log;

    // FC11 — fn(ctx, out_payload_bytes, out_byte_count) -> i32
    public IntPtr report_server_id;
}
