# `ModbusRs` — .NET (C#) bindings for `modbus-rs`

`ModbusRs` is the managed C# wrapper around the native `mbus_ffi` cdylib.
It exposes the [`mbus-client-async`](../../mbus-client-async/) Modbus
TCP client through a `Task`-based async API on top of `[LibraryImport]`
P/Invoke declarations.

> **Status:** Phases 1–4 complete — TCP client (all standard FCs), serial
> client (RTU + ASCII), TCP server (vtable-based request handler), and TCP
> gateway (unit-ID routing to downstream servers).

## Layout

```
mbus-ffi/dotnet/
├── ModbusRs/                    # The library (net8.0)
│   ├── ModbusTcpClient.cs       # Async TCP Modbus client (all standard FCs)
│   ├── ModbusSerialClient.cs    # Async serial Modbus client (RTU / ASCII)
│   ├── ModbusTcpServer.cs       # Modbus TCP server with vtable dispatch
│   ├── ModbusTcpGateway.cs      # Modbus TCP gateway (unit-ID routing)
│   ├── ModbusRequestHandler.cs  # Abstract base for server request handlers
│   ├── ModbusExceptionCode.cs   # Modbus exception code enum
│   ├── ModbusServerException.cs # Exception for server-side Modbus errors
│   ├── ModbusStatus.cs          # Status enum (mirrors C MbusStatusCode)
│   ├── ModbusException.cs       # Exception type carrying the status code
│   └── Native/
│       ├── NativeMethods.cs         # [LibraryImport] declarations
│       ├── SafeTcpClientHandle.cs   # SafeHandle for TCP client pointer
│       ├── SafeSerialClientHandle.cs # SafeHandle for serial client pointer
│       └── MbusDnServerVtable.cs   # [StructLayout] vtable for server callbacks
├── ModbusRs.Tests/              # xUnit tests
│   ├── ModbusServerFixture.cs   # IClassFixture: launches Rust example server
│   ├── ModbusTcpClientTests.cs  # Server-less validation / lifetime tests
│   └── ModbusTcpClientRoundTripTests.cs  # End-to-end round-trip tests
└── ModbusRs.slnx                # Solution descriptor
```

## Building

You need the .NET 8 SDK and a working Rust toolchain.

```bash
# 1. Build the native cdylib (produces target/debug/libmbus_ffi.{so,dylib} or
#    target/debug/mbus_ffi.dll, plus the C header at
#    target/mbus-ffi/include/modbus_rs_dotnet.h).
cargo build -p mbus-ffi --features dotnet,registers

# 2. Build the example test server used by the C# round-trip tests.
cargo build -p mbus-ffi --example dotnet_test_server --features dotnet,registers

# 3. Build the managed library.
dotnet build mbus-ffi/dotnet/ModbusRs/ModbusRs.csproj
```

The test project's `.csproj` automatically copies the freshly built
`libmbus_ffi.{so|dylib|dll}` from `target/debug/` next to the test
assembly, so `[LibraryImport("mbus_ffi")]` resolves without any
extra path setup.

## Testing

```bash
# Make sure both Rust artefacts are up to date first.
cargo build -p mbus-ffi --features dotnet,registers
cargo build -p mbus-ffi --example dotnet_test_server --features dotnet,registers

# Then run the C# test suite.
dotnet test mbus-ffi/dotnet/ModbusRs.Tests/ModbusRs.Tests.csproj
```

The Rust integration test that exercises the same FFI surface lives at
[`mbus-ffi/tests/dotnet_tcp_client.rs`](../tests/dotnet_tcp_client.rs)
and runs as part of `cargo test -p mbus-ffi --features dotnet`.

## Quick start

```csharp
using ModbusRs;

await using var _placeholder = default(IDisposable); // C# 8+ pattern; see Dispose below

using var client = new ModbusTcpClient("192.168.1.10", 502);
client.SetRequestTimeout(TimeSpan.FromSeconds(2));

await client.ConnectAsync();

ushort[] regs = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);
await client.WriteSingleRegisterAsync(unitId: 1, address: 5, value: 0xBEEF);
await client.WriteMultipleRegistersAsync(unitId: 1, address: 8, new ushort[] { 1, 2, 3 });

await client.DisconnectAsync();
```

## Design notes

* **Ownership.** Each `ModbusTcpClient` owns a `SafeHandle` that wraps
  the opaque `*mut MbusDnTcpClient` returned by `mbus_dn_tcp_client_new`.
  The handle's `ReleaseHandle()` calls `mbus_dn_tcp_client_free`,
  guaranteeing the native destructor runs even if the user forgets
  `Dispose()`.
* **Threading.** The native entry points block the calling thread on a
  shared Tokio runtime; the managed wrapper hides this behind
  `Task.Run`. This is binary-compatible with a future swap to a true
  completion-callback implementation.
* **Errors.** Every native call returns a `ModbusStatus`; non-`Ok`
  values are surfaced as a `ModbusException` carrying the original
  status code in the `Status` property.
* **No reuse of C-binding callback infra.** The managed surface uses
  only the `mbus_dn_*` family of entry points. The PyO3- and
  C-static-pool-based bindings are independent.

## Roadmap

This crate is currently in **Phase 1**. See the parent PR description
for the full multi-phase plan covering the rest of the FCs, serial
transport, server, and gateway bindings.
