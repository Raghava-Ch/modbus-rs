using System;

namespace ModbusRs;

/// <summary>
/// Describes a single read sub-request for
/// <see cref="ModbusTcpClient.ReadFileRecordAsync"/> and
/// <see cref="ModbusSerialClient.ReadFileRecordAsync"/>.
/// </summary>
public sealed class FileRecordSubRequest
{
    /// <summary>File number (1–65535).</summary>
    public ushort FileNumber { get; }
    /// <summary>Starting record number within the file (0–9999).</summary>
    public ushort RecordNumber { get; }
    /// <summary>Number of 16-bit registers to read (1–125).</summary>
    public ushort RecordLength { get; }

    /// <summary>Creates a new read sub-request.</summary>
    /// <param name="fileNumber">File number (1–65535).</param>
    /// <param name="recordNumber">Starting record number.</param>
    /// <param name="recordLength">Number of 16-bit registers to read.</param>
    public FileRecordSubRequest(ushort fileNumber, ushort recordNumber, ushort recordLength)
    {
        if (fileNumber == 0)
            throw new ArgumentOutOfRangeException(nameof(fileNumber), "File number must be 1–65535.");
        if (recordLength == 0)
            throw new ArgumentOutOfRangeException(nameof(recordLength), "Record length must be > 0.");

        FileNumber = fileNumber;
        RecordNumber = recordNumber;
        RecordLength = recordLength;
    }
}

/// <summary>
/// Describes a single write sub-request for
/// <see cref="ModbusTcpClient.WriteFileRecordAsync"/> and
/// <see cref="ModbusSerialClient.WriteFileRecordAsync"/>.
/// </summary>
public sealed class FileRecordWriteSubRequest
{
    /// <summary>File number (1–65535).</summary>
    public ushort FileNumber { get; }
    /// <summary>Starting record number within the file (0–9999).</summary>
    public ushort RecordNumber { get; }
    /// <summary>Register data to write (must contain at least one word).</summary>
    public ushort[] Data { get; }

    /// <summary>Creates a new write sub-request.</summary>
    /// <param name="fileNumber">File number (1–65535).</param>
    /// <param name="recordNumber">Starting record number.</param>
    /// <param name="data">Register data to write.</param>
    public FileRecordWriteSubRequest(ushort fileNumber, ushort recordNumber, ushort[] data)
    {
        if (fileNumber == 0)
            throw new ArgumentOutOfRangeException(nameof(fileNumber), "File number must be 1–65535.");
        ArgumentNullException.ThrowIfNull(data);
        if (data.Length == 0)
            throw new ArgumentException("Data must contain at least one word.", nameof(data));
        if (data.Length > 125)
            throw new ArgumentException("Data must not exceed 125 words.", nameof(data));

        FileNumber = fileNumber;
        RecordNumber = recordNumber;
        Data = data;
    }
}
