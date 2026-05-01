# .NET (C#) Bindings

This page covers the `ModbusRs` managed library — the C# wrapper around the native
`mbus_ffi` cdylib.  It provides a Task-based async API for Modbus TCP client, serial
client, TCP server, and TCP gateway, all via P/Invoke over `[LibraryImport]`.

---

## Contents

- [Requirements](#requirements)
- [Building the Native Library](#building-the-native-library)
- [Native DLL Deployment](#native-dll-deployment)
  - [Example projects (auto-copy)](#example-projects-auto-copy)
  - [Your own project](#your-own-project)
  - [Troubleshooting DllNotFoundException](#troubleshooting-dllnotfoundexception)
- [Building the Managed Library](#building-the-managed-library)
- [Visual Studio 2022 Setup](#visual-studio-2022-setup)
- [Quick Start](#quick-start)
  - [TCP Client](#tcp-client)
  - [Serial Client (RTU / ASCII)](#serial-client-rtu--ascii)
  - [TCP Server](#tcp-server)
  - [TCP Gateway](#tcp-gateway)
- [API Reference](#api-reference)
- [Error Handling](#error-handling)
- [Testing](#testing)
- [Examples](#examples)
- [Feature Flags](#feature-flags)
- [Design Notes](#design-notes)

---

## Requirements

| Tool | Minimum version |
|------|----------------|
| Rust toolchain (`rustup`) | stable |
| .NET SDK | 8.0 |
| Visual Studio 2022 (Windows) | 17.x with ".NET desktop development" workload |
| cargo-cbindgen (optional, header gen) | any |

---

## Building the Native Library

The managed library is a thin P/Invoke wrapper.  The native `mbus_ffi` cdylib must
be compiled with Rust before any .NET build or test can succeed.

```bash
# Debug build (default — required for VS 2022 "Debug" configuration)
cargo build -p mbus-ffi --features dotnet,full

# Release build (required for VS 2022 "Release" configuration)
cargo build --release -p mbus-ffi --features dotnet,full
```

The `full` feature enables all Modbus function codes (coils, registers,
discrete-inputs, FIFO, file-record, diagnostics).  If you only need a subset,
substitute `full` with a comma-separated list:

```bash
# Only holding registers and coils
cargo build -p mbus-ffi --features dotnet,registers,coils
```

Output artefacts by platform:

| Platform | Debug | Release |
|----------|-------|---------|
| Windows | `target\debug\mbus_ffi.dll` | `target\release\mbus_ffi.dll` |
| Linux | `target/debug/libmbus_ffi.so` | `target/release/libmbus_ffi.so` |
| macOS | `target/debug/libmbus_ffi.dylib` | `target/release/libmbus_ffi.dylib` |

---

## Native DLL Deployment

`[LibraryImport("mbus_ffi")]` instructs the .NET runtime to load `mbus_ffi.dll`
(Windows) or `libmbus_ffi.so`/`.dylib` (Linux/macOS) **from a directory on the
native library search path**.  The .NET runtime checks, in order:

1. The application's output directory (e.g. `bin\Debug\net8.0\`)
2. Directories listed in `PATH` (Windows) or `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`
3. The working directory

The simplest and most reliable approach is to copy the native artefact into the
output directory, which the example projects do automatically via MSBuild.

### Example projects (auto-copy)

Both example projects (`ModbusRsClientExample` and `ModbusRsServerExample`) already
contain MSBuild items that resolve the path from the Cargo `target\` tree and copy
the correct artefact to the output directory on every build:

```xml
<!-- Inside ModbusRsClientExample.csproj -->
<CargoProfile Condition="'$(Configuration)' == 'Release'">release</CargoProfile>
<CargoProfile Condition="'$(CargoProfile)' == ''">debug</CargoProfile>
<NativeDllDir>$(MSBuildThisFileDirectory)..\..\..\..\target\$(CargoProfile)\</NativeDllDir>

<NativeLibName Condition="$([MSBuild]::IsOSPlatform('Windows'))">mbus_ffi.dll</NativeLibName>
<NativeLibName Condition="$([MSBuild]::IsOSPlatform('Linux'))">libmbus_ffi.so</NativeLibName>
<NativeLibName Condition="$([MSBuild]::IsOSPlatform('OSX'))">libmbus_ffi.dylib</NativeLibName>

<None Include="$(NativeDllDir)$(NativeLibName)"
      Condition="'$(NativeLibName)' != '' And Exists('$(NativeDllDir)$(NativeLibName)')">
  <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
  <Link>$(NativeLibName)</Link>
  <Visible>false</Visible>
</None>
```

The `Condition="Exists(...)"` guard means the project still builds cleanly if the
Cargo build has not been run yet — but launching the executable will throw
`DllNotFoundException` until the native library is present.

### Your own project

If you create a new project that references `ModbusRs`, add the same snippet to
your `.csproj`, adjusting the relative path depth from your project file to the
workspace root:

```xml
<PropertyGroup>
  <CargoProfile Condition="'$(Configuration)' == 'Release'">release</CargoProfile>
  <CargoProfile Condition="'$(CargoProfile)' == ''">debug</CargoProfile>
  <!-- Adjust the number of ..\  segments to reach the repo root -->
  <NativeDllDir>$(MSBuildThisFileDirectory)..\..\..\target\$(CargoProfile)\</NativeDllDir>
  <NativeLibName Condition="$([MSBuild]::IsOSPlatform('Windows'))">mbus_ffi.dll</NativeLibName>
  <NativeLibName Condition="$([MSBuild]::IsOSPlatform('Linux'))">libmbus_ffi.so</NativeLibName>
  <NativeLibName Condition="$([MSBuild]::IsOSPlatform('OSX'))">libmbus_ffi.dylib</NativeLibName>
</PropertyGroup>

<ItemGroup>
  <None Include="$(NativeDllDir)$(NativeLibName)"
        Condition="'$(NativeLibName)' != '' And Exists('$(NativeDllDir)$(NativeLibName)')">
    <CopyToOutputDirectory>PreserveNewest</CopyToOutputDirectory>
    <Link>$(NativeLibName)</Link>
    <Visible>false</Visible>
  </None>
</ItemGroup>
```

Alternatively, as a one-time manual step, copy the DLL next to your `.exe`:

```powershell
# Windows (PowerShell) — Debug build
Copy-Item target\debug\mbus_ffi.dll `
  mbus-ffi\dotnet\examples\ModbusRsClientExample\bin\Debug\net8.0\

# Windows (PowerShell) — Release build
Copy-Item target\release\mbus_ffi.dll `
  mbus-ffi\dotnet\examples\ModbusRsClientExample\bin\Release\net8.0\
```

```bash
# Linux / macOS — Debug build
cp target/debug/libmbus_ffi.so   \
   mbus-ffi/dotnet/examples/ModbusRsClientExample/bin/Debug/net8.0/
# macOS uses .dylib
cp target/debug/libmbus_ffi.dylib \
   mbus-ffi/dotnet/examples/ModbusRsClientExample/bin/Debug/net8.0/
```

### Troubleshooting DllNotFoundException

**Symptom**

```
System.DllNotFoundException: Unable to load DLL 'mbus_ffi' or one of its
dependencies: The specified module could not be found. (0x8007007E)
```

**Checklist**

1. **Did you run `cargo build` first?**  
   The native library does not ship with the repository. Run
   `cargo build -p mbus-ffi --features dotnet,full` before opening Visual Studio.

2. **Does the configuration match?**  
   A Debug Cargo build (`target\debug\mbus_ffi.dll`) is used by the VS **Debug**
   configuration; a `--release` build (`target\release\`) is used by **Release**.
   Mixing them (e.g. running the Release .exe with only a debug DLL present) causes
   the auto-copy item to find nothing and the DLL will be missing.

3. **Is the DLL in the output folder?**  
   After building in VS check that `mbus_ffi.dll` appears in
   `bin\Debug\net8.0\` (or `bin\Release\net8.0\` for Release). If it is missing,
   the MSBuild item silently skipped it because the Cargo artefact did not exist at
   build time. Re-run Cargo, then rebuild the .NET project.

4. **Architecture mismatch (x86 vs x64)?**  
   Cargo defaults to the host machine's native architecture (usually x64).
   If your VS project targets `x86` (Any CPU with Prefer 32-bit enabled), the
   64-bit DLL will load and immediately crash or refuse to load. Keep both projects
   targeting `x64` (or `AnyCPU` with Prefer 32-bit disabled).

5. **Missing Visual C++ Redistributable (Windows)?**  
   The Rust-compiled DLL links against the MSVC runtime. If the target machine
   does not have the Visual C++ 2019/2022 Redistributable installed, Windows
   will report `0x8007007E` even though `mbus_ffi.dll` itself is present.
   Install [vc_redist.x64.exe](https://aka.ms/vs/17/release/vc_redist.x64.exe).

6. **Anti-virus / Windows Defender?**  
   Occasionally a freshly compiled Rust DLL is quarantined on first load.
   Check Windows Security → Protection history if all other steps pass.

---

## Building the Managed Library

```bash
# From the repository root
dotnet build mbus-ffi/dotnet/ModbusRs/ModbusRs.csproj

# Build the full solution (library + tests + examples)
dotnet build mbus-ffi/dotnet/ModbusRs.sln
```

---

## Visual Studio 2022 Setup

1. **Build the native library first** (must be done every time you switch
   between Debug and Release, or after changing Rust source):

   ```powershell
   # Debug
   cargo build -p mbus-ffi --features dotnet,full

   # Release
   cargo build --release -p mbus-ffi --features dotnet,full
   ```

2. **Open the solution** in Visual Studio 2022:

   ```
   File → Open → Project/Solution
   → <repo root>\mbus-ffi\dotnet\ModbusRs.sln
   ```

3. **Select the configuration** (Debug or Release) in the toolbar — it must
   match the Cargo build you ran in step 1.

4. **Build the solution** (`Ctrl+Shift+B` or Build → Build Solution).  
   MSBuild will copy `mbus_ffi.dll` from `target\debug\` (or `target\release\`)
   into each project's output folder automatically.

5. **Run or debug** (`F5`).  
   The DLL is now present in `bin\<Configuration>\net8.0\`, so P/Invoke resolves.

> **Tip:** Add `cargo build -p mbus-ffi --features dotnet,full` as a
> Pre-Build Event in Visual Studio  
> (**Project Properties → Build Events → Pre-build event command line**) to
> keep the native library in sync automatically on every VS build:
>
> ```
> cargo build -p mbus-ffi --features dotnet,full
> ```
>
> For Release builds change the command to include `--release`.

---

## Quick Start

### TCP Client

```csharp
using ModbusRs;

using var client = new ModbusTcpClient("192.168.1.10", 502);
client.SetRequestTimeout(TimeSpan.FromSeconds(2));

await client.ConnectAsync();

// Read holding registers
ushort[] regs = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);
Console.WriteLine(string.Join(", ", regs));

// Write a single register
await client.WriteSingleRegisterAsync(unitId: 1, address: 5, value: 0xBEEF);

// Write multiple registers
await client.WriteMultipleRegistersAsync(unitId: 1, address: 8, new ushort[] { 1, 2, 3 });

// Read coils
bool[] coils = await client.ReadCoilsAsync(unitId: 1, address: 0, quantity: 8);

// Write a single coil
await client.WriteSingleCoilAsync(unitId: 1, address: 0, value: true);

await client.DisconnectAsync();
```

### Serial Client (RTU / ASCII)

```csharp
using ModbusRs;

using var client = new ModbusSerialClient(
    port:     "COM3",       // Linux: "/dev/ttyUSB0"
    baudRate: 9600,
    unitId:   1,
    mode:     SerialMode.Rtu);

client.SetRequestTimeout(TimeSpan.FromSeconds(2));
await client.ConnectAsync();

ushort[] regs = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);

await client.DisconnectAsync();
```

### TCP Server

```csharp
using ModbusRs;

// Derive from ModbusRequestHandler and override the FCs you want to serve.
class MyHandler : ModbusRequestHandler
{
    private ushort[] _regs = new ushort[100];

    public override ushort[] ReadHoldingRegisters(byte unitId, ushort address, ushort quantity)
        => _regs[address..(address + quantity)];

    public override void WriteSingleRegister(byte unitId, ushort address, ushort value)
        => _regs[address] = value;

    public override void WriteMultipleRegisters(byte unitId, ushort address, ushort[] values)
    {
        for (int i = 0; i < values.Length; i++)
            _regs[address + i] = values[i];
    }
}

// Listen on all interfaces, port 502.
using var server = new ModbusTcpServer("0.0.0.0", 502, new MyHandler(), unitId: 1);
await server.StartAsync();

Console.WriteLine("Server listening. Press Ctrl+C to stop.");
await Task.Delay(Timeout.Infinite);

await server.StopAsync();
```

### TCP Gateway

```csharp
using ModbusRs;

// Route unit IDs 1–10 to a downstream server at 192.168.1.20:502.
using var gateway = new ModbusTcpGateway("0.0.0.0", 5020);
gateway.AddRoute(unitIdMin: 1, unitIdMax: 10,
                 downstreamHost: "192.168.1.20", downstreamPort: 502);

await gateway.StartAsync();
Console.WriteLine("Gateway running. Press Ctrl+C to stop.");
await Task.Delay(Timeout.Infinite);
await gateway.StopAsync();
```

---

## API Reference

### `ModbusTcpClient`

| Method | Description |
|--------|-------------|
| `ConnectAsync()` | Open TCP connection to the Modbus server |
| `DisconnectAsync()` | Close the connection |
| `SetRequestTimeout(TimeSpan)` | Per-request response timeout |
| `ReadCoilsAsync(unitId, address, quantity)` | FC01 |
| `ReadDiscreteInputsAsync(unitId, address, quantity)` | FC02 |
| `ReadHoldingRegistersAsync(unitId, address, quantity)` | FC03 |
| `ReadInputRegistersAsync(unitId, address, quantity)` | FC04 |
| `WriteSingleCoilAsync(unitId, address, value)` | FC05 |
| `WriteSingleRegisterAsync(unitId, address, value)` | FC06 |
| `ReadExceptionStatusAsync(unitId)` | FC07 |
| `DiagnosticsAsync(unitId, subFunction, data)` | FC08 |
| `WriteMultipleCoilsAsync(unitId, address, values)` | FC0F |
| `WriteMultipleRegistersAsync(unitId, address, values)` | FC10 |
| `MaskWriteRegisterAsync(unitId, address, andMask, orMask)` | FC16 |
| `ReadWriteMultipleRegistersAsync(unitId, readAddr, readQty, writeAddr, values)` | FC17 |
| `ReadFifoQueueAsync(unitId, fifoPointer)` | FC18 |
| `ReadFileRecordAsync(unitId, subRequests)` | FC14 |
| `WriteFileRecordAsync(unitId, subRequests)` | FC15 |

### `ModbusSerialClient`

Same FC methods as `ModbusTcpClient`, constructed with port name, baud rate,
unit ID, and serial mode (`SerialMode.Rtu` or `SerialMode.Ascii`).

### `ModbusTcpServer`

| Method | Description |
|--------|-------------|
| `StartAsync()` | Begin accepting client connections |
| `StopAsync()` | Gracefully shut down the listener |

Override any of the following in your `ModbusRequestHandler` subclass:

| Virtual method | FC |
|----------------|----|
| `ReadCoils` | FC01 |
| `ReadDiscreteInputs` | FC02 |
| `ReadHoldingRegisters` | FC03 |
| `ReadInputRegisters` | FC04 |
| `WriteSingleCoil` | FC05 |
| `WriteSingleRegister` | FC06 |
| `WriteMultipleCoils` | FC0F |
| `WriteMultipleRegisters` | FC10 |
| `MaskWriteRegister` | FC16 |
| `ReadFifoQueue` | FC18 |
| `ReadFileRecord` | FC14 |
| `WriteFileRecord` | FC15 |

Unimplemented FCs automatically return a Modbus "Illegal Function" exception
response to the client.

### `ModbusTcpGateway`

| Method | Description |
|--------|-------------|
| `AddRoute(unitIdMin, unitIdMax, downstreamHost, downstreamPort)` | Register a downstream route |
| `StartAsync()` | Start accepting upstream connections |
| `StopAsync()` | Stop the gateway |

---

## Error Handling

Every `*Async` method throws `ModbusException` on failure.  The `Status` property
carries the underlying `ModbusStatus` code:

```csharp
try
{
    await client.ConnectAsync();
    ushort[] regs = await client.ReadHoldingRegistersAsync(1, 0, 4);
}
catch (ModbusException ex) when (ex.Status == ModbusStatus.Timeout)
{
    Console.Error.WriteLine("Request timed out");
}
catch (ModbusException ex)
{
    Console.Error.WriteLine($"Modbus error: {ex.Status}");
}
```

Server-side FC handler methods can throw `ModbusServerException` to return a
specific Modbus exception code to the requesting client:

```csharp
public override ushort[] ReadHoldingRegisters(byte unitId, ushort address, ushort quantity)
{
    if (address + quantity > _regs.Length)
        throw new ModbusServerException(ModbusExceptionCode.IllegalDataAddress);
    return _regs[address..(address + quantity)];
}
```

---

## Testing

The `ModbusRs.Tests` xUnit project runs both standalone lifetime tests and
end-to-end round-trip tests that spin up the Rust example server in-process.

```bash
# Build native artefacts first
cargo build -p mbus-ffi --features dotnet,full
cargo build -p mbus-ffi --example dotnet_test_server --features dotnet,full

# Run the managed test suite
dotnet test mbus-ffi/dotnet/ModbusRs.Tests/ModbusRs.Tests.csproj
```

The `ModbusRs.Tests.csproj` contains the same native-copy MSBuild items as the
example projects, so `dotnet test` works without any manual DLL placement.

A complementary Rust-side test lives at `mbus-ffi/tests/dotnet_tcp_client.rs` and
runs as part of `cargo test -p mbus-ffi --features dotnet`.

---

## Examples

Runnable example projects are located in:

```
mbus-ffi/dotnet/examples/
├── ModbusRsClientExample/   # TCP + serial client, all supported FCs
└── ModbusRsServerExample/   # TCP server with a concrete request handler
```

Build and run from Visual Studio 2022 (see [Visual Studio 2022 Setup](#visual-studio-2022-setup))
or from the command line:

```bash
# 1. Build the native library (once per Cargo source change)
cargo build -p mbus-ffi --features dotnet,full

# 2. Build and run the client example
dotnet run --project mbus-ffi/dotnet/examples/ModbusRsClientExample

# 3. Build and run the server example
dotnet run --project mbus-ffi/dotnet/examples/ModbusRsServerExample
```

---

## Feature Flags

The native library must be built with the feature flags that match the API you
call from managed code.  The recommended set for full functionality is
`dotnet,full`.

| Cargo feature | Enables |
|---------------|---------|
| `dotnet` | .NET P/Invoke entry points (`mbus_dn_*`) |
| `full` | All FC models: coils, registers, discrete-inputs, FIFO, file-record, diagnostics |
| `registers` | FC03, FC04, FC06, FC10 only |
| `coils` | FC01, FC05, FC0F only |

If you build with only `dotnet,registers` and then call `ReadCoilsAsync`, the
native function will not be exported and you will receive a `ModbusException`
with status `IllegalFunction` at runtime (not at compile time).

---

## Design Notes

* **Ownership.** Each client and server object owns a `SafeHandle` wrapping an
  opaque Rust pointer. `ReleaseHandle()` calls the matching Rust destructor,
  ensuring clean-up even if `Dispose()` is forgotten.

* **Threading.** Native entry points block the calling thread on a shared Tokio
  runtime.  `Task.Run` is used internally so callers see a true async API without
  starving the ThreadPool.

* **No C-pool infrastructure.** The managed surface uses only the `mbus_dn_*`
  entry-point family, which is independent of the C static-pool and PyO3
  binding paths.

* **ABI stability.** The `mbus_dn_*` symbols use a flat, versioned C ABI. As long
  as you rebuild both the Rust library and the managed wrapper from the same
  source revision, ABI compatibility is guaranteed.

---

## Additional Reference

- [`mbus-ffi/dotnet/README.md`](../mbus-ffi/dotnet/README.md) — library layout,
  full build commands, test instructions
- [`mbus-ffi/README.md`](../mbus-ffi/README.md) — the `mbus-ffi` crate reference
  (C, WASM, Python, and .NET bindings overview)
- [`documentation/client/c_bindings.md`](client/c_bindings.md) — native C client
  binding reference
- [`documentation/python_bindings.md`](python_bindings.md) — Python binding
  reference
