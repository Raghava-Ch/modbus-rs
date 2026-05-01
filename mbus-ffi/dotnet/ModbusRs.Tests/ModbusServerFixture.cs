using System;
using System.Diagnostics;
using System.IO;
using System.Threading;

namespace ModbusRs.Tests;

/// <summary>
/// xUnit class fixture that launches the Rust example
/// <c>dotnet_test_server</c> as a child process and exposes the bound
/// TCP port. Disposed once when the test class completes.
/// </summary>
public sealed class ModbusServerFixture : IDisposable
{
    public ushort Port { get; }
    private readonly Process _process;

    public ModbusServerFixture()
    {
        // The example binary is built by `cargo build --example
        // dotnet_test_server --features dotnet,registers`. CI scripts run
        // this before `dotnet test`; locally, run it once.
        string repoRoot = LocateRepoRoot();
        string binary = Path.Combine(repoRoot, "target", "debug", "examples",
            OperatingSystem.IsWindows() ? "dotnet_test_server.exe" : "dotnet_test_server");

        if (!File.Exists(binary))
        {
            throw new InvalidOperationException(
                $"Test server binary not found at '{binary}'.\n" +
                "Run: cargo build -p mbus-ffi --example dotnet_test_server --features dotnet,registers");
        }

        var psi = new ProcessStartInfo(binary)
        {
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            CreateNoWindow = true,
        };
        _process = Process.Start(psi)
            ?? throw new InvalidOperationException($"Failed to launch '{binary}'");

        // First line of stdout is `LISTENING <port>`.
        string? line = _process.StandardOutput.ReadLine();
        if (line is null || !line.StartsWith("LISTENING ", StringComparison.Ordinal))
        {
            try { _process.Kill(true); } catch { /* ignore */ }
            throw new InvalidOperationException(
                $"Test server did not announce its port; got: '{line ?? "<eof>"}'");
        }
        if (!ushort.TryParse(line.AsSpan("LISTENING ".Length), out var port))
        {
            try { _process.Kill(true); } catch { /* ignore */ }
            throw new InvalidOperationException($"Could not parse port from: '{line}'");
        }
        Port = port;
    }

    public void Dispose()
    {
        if (!_process.HasExited)
        {
            try { _process.Kill(entireProcessTree: true); } catch { /* ignore */ }
            // Give it a brief moment to release the port.
            _process.WaitForExit(2_000);
        }
        _process.Dispose();
    }

    /// <summary>
    /// Walks up from the test assembly directory until we find the
    /// workspace root (identified by `Cargo.toml` containing
    /// `[workspace]`).
    /// </summary>
    private static string LocateRepoRoot()
    {
        string? dir = AppContext.BaseDirectory;
        while (dir is not null)
        {
            string candidate = Path.Combine(dir, "Cargo.toml");
            if (File.Exists(candidate))
            {
                string text = File.ReadAllText(candidate);
                if (text.Contains("[workspace]"))
                {
                    return dir;
                }
            }
            dir = Path.GetDirectoryName(dir);
        }
        throw new InvalidOperationException(
            "Could not find workspace root (looking for `[workspace]` in Cargo.toml).");
    }
}
