using System;

namespace ModbusRs;

/// <summary>
/// Base class for Modbus server request handlers. Override any method to
/// support that function code; the default implementation throws
/// <see cref="ModbusServerException"/> with
/// <see cref="ModbusExceptionCode.IllegalFunction"/>.
/// </summary>
/// <remarks>
/// <para>
/// Pass an instance to <see cref="ModbusTcpServer"/> which will route
/// incoming requests to the overridden methods.
/// </para>
/// <para>
/// All overrides are called on a background thread managed by the native
/// Tokio runtime. Implementations must be thread-safe.
/// </para>
/// </remarks>
public abstract class ModbusRequestHandler
{
    // ── FC01 ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Handles a Read Coils (FC01) request.
    /// </summary>
    /// <param name="address">Starting coil address.</param>
    /// <param name="count">Number of coils to read.</param>
    /// <returns>Array of <paramref name="count"/> coil values.</returns>
    public virtual bool[] ReadCoils(ushort address, ushort count)
        => throw NotSupported();

    // ── FC05 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Write Single Coil (FC05) request.</summary>
    public virtual void WriteSingleCoil(ushort address, bool value)
        => throw NotSupported();

    // ── FC0F ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Write Multiple Coils (FC0F) request.</summary>
    /// <param name="address">Starting coil address.</param>
    /// <param name="values">One element per coil.</param>
    public virtual void WriteMultipleCoils(ushort address, bool[] values)
        => throw NotSupported();

    // ── FC02 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read Discrete Inputs (FC02) request.</summary>
    public virtual bool[] ReadDiscreteInputs(ushort address, ushort count)
        => throw NotSupported();

    // ── FC03 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read Holding Registers (FC03) request.</summary>
    public virtual ushort[] ReadHoldingRegisters(ushort address, ushort count)
        => throw NotSupported();

    // ── FC04 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read Input Registers (FC04) request.</summary>
    public virtual ushort[] ReadInputRegisters(ushort address, ushort count)
        => throw NotSupported();

    // ── FC06 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Write Single Register (FC06) request.</summary>
    public virtual void WriteSingleRegister(ushort address, ushort value)
        => throw NotSupported();

    // ── FC10 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Write Multiple Registers (FC10) request.</summary>
    public virtual void WriteMultipleRegisters(ushort address, ushort[] values)
        => throw NotSupported();

    // ── FC22 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Mask Write Register (FC22) request.</summary>
    public virtual void MaskWriteRegister(ushort address, ushort andMask, ushort orMask)
        => throw NotSupported();

    // ── FC23 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read/Write Multiple Registers (FC23) request.</summary>
    /// <param name="readAddress">Starting address for the read.</param>
    /// <param name="readCount">Number of registers to read.</param>
    /// <param name="writeAddress">Starting address for the write.</param>
    /// <param name="writeValues">Register values to write.</param>
    /// <returns>Register values read from <paramref name="readAddress"/>.</returns>
    public virtual ushort[] ReadWriteMultipleRegisters(
        ushort readAddress, ushort readCount,
        ushort writeAddress, ushort[] writeValues)
        => throw NotSupported();

    // ── FC24 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read FIFO Queue (FC24) request.</summary>
    public virtual ushort[] ReadFifoQueue(ushort pointerAddress)
        => throw NotSupported();

    // ── FC07 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Read Exception Status (FC07) request.</summary>
    public virtual byte ReadExceptionStatus()
        => throw NotSupported();

    // ── FC0B ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Get Comm Event Counter (FC0B) request.</summary>
    public virtual (ushort Status, ushort EventCount) GetCommEventCounter()
        => throw NotSupported();

    // ── FC0C ─────────────────────────────────────────────────────────────

    /// <summary>
    /// Handles a Get Comm Event Log (FC0C) request.
    /// </summary>
    /// <returns>
    /// Raw event-log payload bytes in the format
    /// <c>[status_hi, status_lo, event_count_hi, event_count_lo,
    /// msg_count_hi, msg_count_lo, event_bytes…]</c>.
    /// </returns>
    public virtual byte[] GetCommEventLog()
        => throw NotSupported();

    // ── FC11 ─────────────────────────────────────────────────────────────

    /// <summary>Handles a Report Server ID (FC11) request.</summary>
    public virtual byte[] ReportServerId()
        => throw NotSupported();

    // ── Helpers ──────────────────────────────────────────────────────────

    private static ModbusServerException NotSupported()
        => new(ModbusExceptionCode.IllegalFunction);
}
