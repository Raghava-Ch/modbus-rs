// ModbusRsServerExample — full-featured Modbus TCP server demonstration.
//
// This console application starts a Modbus TCP server that implements every
// function code supported by the ModbusRs .NET bindings.  It uses a simple
// in-memory data model so you can connect any Modbus client to it and try
// all FC codes.
//
// Usage:
//   dotnet run [-- [<host> [<port> [<unitId>]]]]
//
// Default: host=0.0.0.0, port=502, unitId=1
//
// Build the native library first:
//   cargo build -p mbus-ffi --features dotnet,full --release
// Then copy target/release/libmbus_ffi.so (Linux) / mbus_ffi.dll (Windows)
// next to the compiled executable.

using System;
using System.Threading;
using ModbusRs;

string host = args.Length > 0 ? args[0] : "0.0.0.0";
ushort port = args.Length > 1 ? ushort.Parse(args[1]) : (ushort)502;
byte unitId = args.Length > 2 ? byte.Parse(args[2]) : (byte)1;

Console.WriteLine($"Modbus TCP server demo — listening on {host}:{port}, unit {unitId}");
Console.WriteLine("Press Ctrl+C to stop.");
Console.WriteLine();

using var server = new ModbusTcpServer(host, port, unitId, new DemoHandler());
server.Start();
Console.WriteLine("Server started.");

// Block until Ctrl+C.
using var cts = new CancellationTokenSource();
Console.CancelKeyPress += (_, e) =>
{
    e.Cancel = true;
    cts.Cancel();
};

try { cts.Token.WaitHandle.WaitOne(); }
catch (OperationCanceledException) { }

Console.WriteLine("Stopping…");
server.Stop();
Console.WriteLine("Done.");

// ─────────────────────────────────────────────────────────────────────────────

/// <summary>
/// In-memory Modbus server handler that supports all function codes.
/// </summary>
sealed class DemoHandler : ModbusRequestHandler
{
    // 256-element backing stores (address is the array index).
    private readonly bool[]   _coils          = new bool[256];
    private readonly bool[]   _discreteInputs = new bool[256];
    private readonly ushort[] _holdingRegs    = new ushort[256];
    private readonly ushort[] _inputRegs      = new ushort[256];

    // Simple 4-element FIFO (FC18).
    private readonly ushort[] _fifo = { 10, 20, 30, 40 };

    // File storage: fileNumber → (recordNumber → data[])
    private readonly System.Collections.Generic.Dictionary<ushort,
        System.Collections.Generic.Dictionary<ushort, ushort[]>> _files = new();

    public DemoHandler()
    {
        // Pre-populate some values so reads return interesting data.
        for (int i = 0; i < 8; i++) _coils[i] = (i % 2 == 0);
        for (int i = 0; i < 4; i++) _discreteInputs[i] = (i % 3 == 0);
        for (int i = 0; i < 10; i++) _holdingRegs[i] = (ushort)(i * 100);
        for (int i = 0; i < 4; i++) _inputRegs[i] = (ushort)(i * 10 + 1);

        // File 1, record 0: four words.
        _files[1] = new() { [0] = new ushort[] { 0xAAAA, 0xBBBB, 0xCCCC, 0xDDDD } };
    }

    // ── FC01 — Read Coils ─────────────────────────────────────────────────

    public override bool[] ReadCoils(ushort address, ushort count)
    {
        Console.WriteLine($"  FC01 ReadCoils(addr={address}, count={count})");
        var result = new bool[count];
        for (int i = 0; i < count; i++)
            result[i] = _coils[(address + i) & 0xFF];
        return result;
    }

    // ── FC05 — Write Single Coil ──────────────────────────────────────────

    public override void WriteSingleCoil(ushort address, bool value)
    {
        Console.WriteLine($"  FC05 WriteSingleCoil(addr={address}, value={value})");
        _coils[address & 0xFF] = value;
    }

    // ── FC0F — Write Multiple Coils ───────────────────────────────────────

    public override void WriteMultipleCoils(ushort address, bool[] values)
    {
        Console.WriteLine($"  FC0F WriteMultipleCoils(addr={address}, count={values.Length})");
        for (int i = 0; i < values.Length; i++)
            _coils[(address + i) & 0xFF] = values[i];
    }

    // ── FC02 — Read Discrete Inputs ───────────────────────────────────────

    public override bool[] ReadDiscreteInputs(ushort address, ushort count)
    {
        Console.WriteLine($"  FC02 ReadDiscreteInputs(addr={address}, count={count})");
        var result = new bool[count];
        for (int i = 0; i < count; i++)
            result[i] = _discreteInputs[(address + i) & 0xFF];
        return result;
    }

    // ── FC03 — Read Holding Registers ─────────────────────────────────────

    public override ushort[] ReadHoldingRegisters(ushort address, ushort count)
    {
        Console.WriteLine($"  FC03 ReadHoldingRegisters(addr={address}, count={count})");
        var result = new ushort[count];
        for (int i = 0; i < count; i++)
            result[i] = _holdingRegs[(address + i) & 0xFF];
        return result;
    }

    // ── FC04 — Read Input Registers ───────────────────────────────────────

    public override ushort[] ReadInputRegisters(ushort address, ushort count)
    {
        Console.WriteLine($"  FC04 ReadInputRegisters(addr={address}, count={count})");
        var result = new ushort[count];
        for (int i = 0; i < count; i++)
            result[i] = _inputRegs[(address + i) & 0xFF];
        return result;
    }

    // ── FC06 — Write Single Register ──────────────────────────────────────

    public override void WriteSingleRegister(ushort address, ushort value)
    {
        Console.WriteLine($"  FC06 WriteSingleRegister(addr={address}, value={value})");
        _holdingRegs[address & 0xFF] = value;
    }

    // ── FC10 — Write Multiple Registers ───────────────────────────────────

    public override void WriteMultipleRegisters(ushort address, ushort[] values)
    {
        Console.WriteLine($"  FC10 WriteMultipleRegisters(addr={address}, count={values.Length})");
        for (int i = 0; i < values.Length; i++)
            _holdingRegs[(address + i) & 0xFF] = values[i];
    }

    // ── FC22 — Mask Write Register ────────────────────────────────────────

    public override void MaskWriteRegister(ushort address, ushort andMask, ushort orMask)
    {
        Console.WriteLine($"  FC22 MaskWriteRegister(addr={address}, and=0x{andMask:X4}, or=0x{orMask:X4})");
        var current = _holdingRegs[address & 0xFF];
        _holdingRegs[address & 0xFF] = (ushort)((current & andMask) | (orMask & ~andMask));
    }

    // ── FC23 — Read/Write Multiple Registers ──────────────────────────────

    public override ushort[] ReadWriteMultipleRegisters(
        ushort readAddress, ushort readCount,
        ushort writeAddress, ushort[] writeValues)
    {
        Console.WriteLine($"  FC23 ReadWriteMultipleRegisters(readAddr={readAddress}, readCount={readCount}, writeAddr={writeAddress}, writeCount={writeValues.Length})");
        for (int i = 0; i < writeValues.Length; i++)
            _holdingRegs[(writeAddress + i) & 0xFF] = writeValues[i];
        var result = new ushort[readCount];
        for (int i = 0; i < readCount; i++)
            result[i] = _holdingRegs[(readAddress + i) & 0xFF];
        return result;
    }

    // ── FC18 — Read FIFO Queue ────────────────────────────────────────────

    public override ushort[] ReadFifoQueue(ushort pointerAddress)
    {
        Console.WriteLine($"  FC18 ReadFifoQueue(pointerAddr={pointerAddress})");
        return _fifo;
    }

    // ── FC07 — Read Exception Status ──────────────────────────────────────

    public override byte ReadExceptionStatus()
    {
        Console.WriteLine("  FC07 ReadExceptionStatus()");
        return 0x00;
    }

    // ── FC0B — Get Comm Event Counter ─────────────────────────────────────

    public override (ushort Status, ushort EventCount) GetCommEventCounter()
    {
        Console.WriteLine("  FC0B GetCommEventCounter()");
        return (0x0000, 42);
    }

    // ── FC0C — Get Comm Event Log ─────────────────────────────────────────

    public override byte[] GetCommEventLog()
    {
        Console.WriteLine("  FC0C GetCommEventLog()");
        // Minimal payload: status(2), eventCount(2), messageCount(2), events(0)
        return new byte[] { 0x00, 0x00, 0x00, 0x2A, 0x00, 0x01 };
    }

    // ── FC11 — Report Server ID ───────────────────────────────────────────

    public override byte[] ReportServerId()
    {
        Console.WriteLine("  FC11 ReportServerId()");
        return new byte[] { 0x01, 0xFF, (byte)'M', (byte)'o', (byte)'d', (byte)'b', (byte)'u', (byte)'s' };
    }
}
