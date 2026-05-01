using System;
using System.Threading.Tasks;

namespace ModbusRs.Tests;

/// <summary>
/// Tests that don't need a live Modbus server — exercise the managed
/// wrapper's input validation, lifetime, and exception mapping.
/// </summary>
public class ModbusTcpClientTests
{
    [Fact]
    public void Constructor_NullHost_Throws()
    {
        Assert.Throws<ArgumentNullException>(() => new ModbusTcpClient(null!, 502));
    }

    [Fact]
    public void Constructor_EmptyHost_Throws()
    {
        Assert.Throws<ArgumentException>(() => new ModbusTcpClient(string.Empty, 502));
    }

    [Fact]
    public void Constructor_ValidHost_DoesNotThrow()
    {
        // The constructor only sets up local state; no connection is made.
        using var client = new ModbusTcpClient("127.0.0.1", 0);
        Assert.False(client.HasPendingRequests);
    }

    [Fact]
    public void HasPendingRequests_AfterDispose_Throws()
    {
        var client = new ModbusTcpClient("127.0.0.1", 0);
        client.Dispose();
        Assert.Throws<ObjectDisposedException>(() => client.HasPendingRequests);
    }

    [Fact]
    public async Task ConnectAsync_NoServer_ThrowsConnectionFailed()
    {
        // Port 1 is reliably refused on every loopback interface.
        using var client = new ModbusTcpClient("127.0.0.1", 1);
        client.SetRequestTimeout(TimeSpan.FromSeconds(2));
        var ex = await Assert.ThrowsAsync<ModbusException>(() => client.ConnectAsync());
        // The underlying status may be ConnectionFailed or ConnectionLost depending
        // on platform/race; we just verify it is a connection-class error.
        Assert.True(
            ex.Status is ModbusStatus.ConnectionFailed
                     or ModbusStatus.ConnectionLost
                     or ModbusStatus.IoError,
            $"unexpected status {ex.Status}");
    }

    [Fact]
    public async Task ConnectAsync_AfterDispose_Throws()
    {
        var client = new ModbusTcpClient("127.0.0.1", 0);
        client.Dispose();
        await Assert.ThrowsAsync<ObjectDisposedException>(() => client.ConnectAsync());
    }

    [Fact]
    public async Task WriteMultipleRegistersAsync_EmptyArray_Throws()
    {
        using var client = new ModbusTcpClient("127.0.0.1", 0);
        await Assert.ThrowsAsync<ArgumentException>(
            () => client.WriteMultipleRegistersAsync(1, 0, ReadOnlyMemory<ushort>.Empty));
    }

    [Fact]
    public async Task ReadHoldingRegistersAsync_ZeroQuantity_ReturnsEmpty()
    {
        using var client = new ModbusTcpClient("127.0.0.1", 0);
        var result = await client.ReadHoldingRegistersAsync(1, 0, 0);
        Assert.Empty(result);
    }
}
