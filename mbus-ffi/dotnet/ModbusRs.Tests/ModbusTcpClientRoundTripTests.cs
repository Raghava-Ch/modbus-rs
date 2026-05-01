using System;
using System.Threading.Tasks;

namespace ModbusRs.Tests;

/// <summary>
/// End-to-end round-trip tests against a real <c>AsyncTcpServer</c>
/// launched as a child process by <see cref="ModbusServerFixture"/>.
/// </summary>
public sealed class ModbusTcpClientRoundTripTests : IClassFixture<ModbusServerFixture>
{
    private readonly ModbusServerFixture _server;

    public ModbusTcpClientRoundTripTests(ModbusServerFixture server)
    {
        _server = server;
    }

    [Fact]
    public async Task FullRoundTrip_FC03_FC06_FC16()
    {
        using var client = new ModbusTcpClient("127.0.0.1", _server.Port);
        client.SetRequestTimeout(TimeSpan.FromSeconds(5));
        await client.ConnectAsync();

        // ── FC03: read 4 zeroed registers from a fresh store ─────────────
        var initial = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);
        Assert.Equal(4, initial.Length);
        Assert.All(initial, v => Assert.Equal(0, v));

        // ── FC06: write a single register ────────────────────────────────
        var (echoAddr, echoVal) = await client.WriteSingleRegisterAsync(1, address: 5, value: 0xBEEF);
        Assert.Equal(5, echoAddr);
        Assert.Equal(0xBEEF, echoVal);

        // ── FC16: write three registers ──────────────────────────────────
        ushort[] payload = { 0xAAAA, 0xBBBB, 0xCCCC };
        var (startAddr, qty) = await client.WriteMultipleRegistersAsync(1, address: 8, payload);
        Assert.Equal(8, startAddr);
        Assert.Equal(3, qty);

        // ── FC03: read back and confirm ──────────────────────────────────
        var readback = await client.ReadHoldingRegistersAsync(1, address: 5, quantity: 6);
        Assert.Equal(new ushort[] { 0xBEEF, 0, 0, 0xAAAA, 0xBBBB, 0xCCCC }, readback);

        // ── Disconnect cleanly ───────────────────────────────────────────
        await client.DisconnectAsync();
    }

    [Fact]
    public async Task ReadHoldingRegisters_OutOfRange_ThrowsModbusException()
    {
        using var client = new ModbusTcpClient("127.0.0.1", _server.Port);
        client.SetRequestTimeout(TimeSpan.FromSeconds(5));
        await client.ConnectAsync();

        // The test app exposes 256 registers; address 1000 is illegal.
        var ex = await Assert.ThrowsAsync<ModbusException>(
            () => client.ReadHoldingRegistersAsync(1, address: 1000, quantity: 1));
        Assert.Equal(ModbusStatus.ModbusException, ex.Status);
    }
}
