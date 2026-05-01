// ModbusRsClientExample — full-featured Modbus TCP client demonstration.
//
// This console application shows every function code supported by the
// ModbusRs .NET bindings.  Run a Modbus TCP server on 127.0.0.1:502 (or
// pass a different host and port as command-line arguments) before starting
// this program.
//
// Usage:
//   dotnet run [-- <host> [<port> [<unitId>]]]
//
// Default: host=127.0.0.1, port=502, unitId=1
//
// Build the native library first:
//   cargo build -p mbus-ffi --features dotnet,full --release
// Then copy target/release/libmbus_ffi.so (Linux) / mbus_ffi.dll (Windows)
// next to the compiled executable.

using System;
using System.Threading.Tasks;
using ModbusRs;

string host = args.Length > 0 ? args[0] : "127.0.0.1";
ushort port = args.Length > 1 ? ushort.Parse(args[1]) : (ushort)502;
byte unitId = args.Length > 2 ? byte.Parse(args[2]) : (byte)1;

Console.WriteLine($"Modbus TCP client demo — connecting to {host}:{port}, unit {unitId}");
Console.WriteLine();

using var client = new ModbusTcpClient(host, port);
client.SetRequestTimeout(TimeSpan.FromSeconds(3));

// ── Connect ───────────────────────────────────────────────────────────────

await client.ConnectAsync();
Console.WriteLine("Connected.");

// ── FC03 — Read Holding Registers ────────────────────────────────────────

{
    Console.Write("FC03 ReadHoldingRegisters(addr=0, qty=4) → ");
    var regs = await client.ReadHoldingRegistersAsync(unitId, 0, 4);
    Console.WriteLine(string.Join(", ", regs));
}

// ── FC04 — Read Input Registers ──────────────────────────────────────────

{
    Console.Write("FC04 ReadInputRegisters(addr=0, qty=2)   → ");
    var regs = await client.ReadInputRegistersAsync(unitId, 0, 2);
    Console.WriteLine(string.Join(", ", regs));
}

// ── FC06 — Write Single Register ─────────────────────────────────────────

{
    Console.Write("FC06 WriteSingleRegister(addr=0, val=42) → ");
    var (addr, val) = await client.WriteSingleRegisterAsync(unitId, 0, 42);
    Console.WriteLine($"echo addr={addr}, val={val}");
}

// ── FC16 — Write Multiple Registers ──────────────────────────────────────

{
    Console.Write("FC16 WriteMultipleRegisters(addr=0, [1,2,3,4]) → ");
    var (startAddr, qty) = await client.WriteMultipleRegistersAsync(
        unitId, 0, new ushort[] { 1, 2, 3, 4 }.AsMemory());
    Console.WriteLine($"echo startAddr={startAddr}, qty={qty}");
}

// ── FC22 — Mask Write Register ────────────────────────────────────────────

{
    Console.Write("FC22 MaskWriteRegister(addr=0, and=0xFF00, or=0x0042) → ");
    await client.MaskWriteRegisterAsync(unitId, 0, andMask: 0xFF00, orMask: 0x0042);
    Console.WriteLine("ok");
}

// ── FC23 — Read/Write Multiple Registers ─────────────────────────────────

{
    Console.Write("FC23 ReadWriteMultipleRegisters(read=0,qty=2, write=10,[100,200]) → ");
    var result = await client.ReadWriteMultipleRegistersAsync(
        unitId,
        readAddress: 0, readQuantity: 2,
        writeAddress: 10, writeValues: new ushort[] { 100, 200 }.AsMemory());
    Console.WriteLine(string.Join(", ", result));
}

// ── FC01 — Read Coils ─────────────────────────────────────────────────────

{
    Console.Write("FC01 ReadCoils(addr=0, qty=8)   → ");
    var coils = await client.ReadCoilsAsync(unitId, 0, 8);
    Console.WriteLine(string.Join(", ", coils));
}

// ── FC02 — Read Discrete Inputs ──────────────────────────────────────────

{
    Console.Write("FC02 ReadDiscreteInputs(addr=0, qty=4) → ");
    var inputs = await client.ReadDiscreteInputsAsync(unitId, 0, 4);
    Console.WriteLine(string.Join(", ", inputs));
}

// ── FC05 — Write Single Coil ─────────────────────────────────────────────

{
    Console.Write("FC05 WriteSingleCoil(addr=0, true) → ");
    var (addr, val) = await client.WriteSingleCoilAsync(unitId, 0, true);
    Console.WriteLine($"echo addr={addr}, val={val}");
}

// ── FC0F — Write Multiple Coils ──────────────────────────────────────────

{
    Console.Write("FC0F WriteMultipleCoils(addr=0, [true,false,true,false]) → ");
    var (startAddr, qty) = await client.WriteMultipleCoilsAsync(
        unitId, 0, new[] { true, false, true, false });
    Console.WriteLine($"echo startAddr={startAddr}, qty={qty}");
}

// ── FC18 — Read FIFO Queue ────────────────────────────────────────────────

{
    Console.Write("FC18 ReadFifoQueue(addr=0) → ");
    try
    {
        var fifo = await client.ReadFifoQueueAsync(unitId, 0);
        Console.WriteLine($"[{string.Join(", ", fifo)}]");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC07 — Read Exception Status ─────────────────────────────────────────

{
    Console.Write("FC07 ReadExceptionStatus() → ");
    try
    {
        var exStatus = await client.ReadExceptionStatusAsync(unitId);
        Console.WriteLine($"0x{exStatus:X2}");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC0B — Get Comm Event Counter ────────────────────────────────────────

{
    Console.Write("FC0B GetCommEventCounter() → ");
    try
    {
        var (st, cnt) = await client.GetCommEventCounterAsync(unitId);
        Console.WriteLine($"status=0x{st:X4}, eventCount={cnt}");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC0C — Get Comm Event Log ────────────────────────────────────────────

{
    Console.Write("FC0C GetCommEventLog() → ");
    try
    {
        var log = await client.GetCommEventLogAsync(unitId);
        Console.WriteLine($"{log.Length} bytes");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC11 — Report Server ID ──────────────────────────────────────────────

{
    Console.Write("FC11 ReportServerId() → ");
    try
    {
        var id = await client.ReportServerIdAsync(unitId);
        Console.WriteLine($"{id.Length} bytes: [{string.Join(", ", id)}]");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC08 — Diagnostics (Return Query Data loopback) ──────────────────────

{
    Console.Write("FC08 Diagnostics(subFn=0x0000, data=[0xABCD]) → ");
    try
    {
        var (echoSf, echoData) = await client.DiagnosticsAsync(
            unitId,
            subFunction: 0x0000,     // ReturnQueryData
            data: new ushort[] { 0xABCD }.AsMemory());
        Console.WriteLine($"echoSubFn=0x{echoSf:X4}, data=[{string.Join(", ", Array.ConvertAll(echoData, v => $"0x{v:X4}"))}]");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC14 — Read File Record ───────────────────────────────────────────────

{
    Console.Write("FC14 ReadFileRecord(file=1, rec=0, len=4) → ");
    try
    {
        var words = await client.ReadFileRecordAsync(unitId, new[]
        {
            new FileRecordSubRequest(fileNumber: 1, recordNumber: 0, recordLength: 4),
        });
        Console.WriteLine($"[{string.Join(", ", words)}]");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── FC15 — Write File Record ──────────────────────────────────────────────

{
    Console.Write("FC15 WriteFileRecord(file=1, rec=0, [0x0001,0x0002,0x0003,0x0004]) → ");
    try
    {
        await client.WriteFileRecordAsync(unitId, new[]
        {
            new FileRecordWriteSubRequest(
                fileNumber: 1,
                recordNumber: 0,
                data: new ushort[] { 0x0001, 0x0002, 0x0003, 0x0004 }),
        });
        Console.WriteLine("ok");
    }
    catch (ModbusException ex)
    {
        Console.WriteLine($"(server returned exception: {ex.Status})");
    }
}

// ── Disconnect ────────────────────────────────────────────────────────────

await client.DisconnectAsync();
Console.WriteLine();
Console.WriteLine("Disconnected. Done.");
