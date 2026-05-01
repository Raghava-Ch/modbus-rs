# `ModbusRs` — .NET (C#) bindings for `modbus-rs`

`ModbusRs` is the managed C# wrapper around the native `mbus_ffi` cdylib.
It exposes the [`mbus-client-async`](../../mbus-client-async/) Modbus
TCP client through a `Task`-based async API on top of `[LibraryImport]`
P/Invoke declarations.

Included in the current release:

- **TCP client** — all standard Modbus function codes (FC01–FC18)
- **Serial client** — RTU and ASCII transport
- **TCP server** — vtable-based request handler with all standard FCs
- **TCP gateway** — unit-ID routing to downstream Modbus servers

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

### Step 1 — Build the native library

The `mbus_ffi` native library must be compiled with Rust **before** any .NET
build or debug session.

```bash
# Debug build (use with the VS 2022 "Debug" configuration)
cargo build -p mbus-ffi --features dotnet,full

# Release build (use with the VS 2022 "Release" configuration)
cargo build --release -p mbus-ffi --features dotnet,full
```

Use `full` to enable all Modbus function codes.  If you only need a subset:

```bash
# Holding registers and coils only
cargo build -p mbus-ffi --features dotnet,registers,coils
```

Output artefacts:

| Platform | Debug | Release |
|----------|-------|---------|
| Windows  | `target\debug\mbus_ffi.dll`       | `target\release\mbus_ffi.dll`       |
| Linux    | `target/debug/libmbus_ffi.so`     | `target/release/libmbus_ffi.so`     |
| macOS    | `target/debug/libmbus_ffi.dylib`  | `target/release/libmbus_ffi.dylib`  |

### Step 2 — Build the managed library

```bash
dotnet build mbus-ffi/dotnet/ModbusRs/ModbusRs.csproj
```

Or build the whole solution:

```bash
dotnet build mbus-ffi/dotnet/ModbusRs.sln
```

## Native DLL Deployment

`[LibraryImport("mbus_ffi")]` asks the .NET runtime to load `mbus_ffi.dll`
(Windows) or `libmbus_ffi.so`/`.dylib` (Linux/macOS) from a directory on the
native library search path.  The search order is:

1. The application output directory (`bin\Debug\net8.0\` etc.)
2. `PATH` (Windows) or `LD_LIBRARY_PATH` / `DYLD_LIBRARY_PATH`
3. The working directory

**The example projects copy the DLL automatically.**  Both
`ModbusRsClientExample.csproj` and `ModbusRsServerExample.csproj` contain MSBuild
items that resolve the native artefact from the Cargo `target\` tree and copy it
to the output directory during every build:

```xml
<!-- Excerpt from the example .csproj files -->
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

The `Condition="Exists(...)"` guard keeps the project buildable even if the Cargo
build has not run yet — but launching the executable without the DLL present will
throw `DllNotFoundException`.

### Troubleshooting `DllNotFoundException`

**Symptom**
```
System.DllNotFoundException: Unable to load DLL 'mbus_ffi' or one of its
dependencies: The specified module could not be found. (0x8007007E)
```

**Checklist**

1. **Run `cargo build` before opening Visual Studio.**  
   The native DLL is never committed to the repository. It must be compiled
   from Rust source at least once before the .NET project can run.

2. **Match the configuration to the Cargo build.**  
   Debug VS configuration → `cargo build` (no `--release`).  
   Release VS configuration → `cargo build --release`.  
   A Debug DLL in `target\debug\` will not satisfy a Release build's copy rule.

3. **Verify the DLL is in the output folder.**  
   After a successful VS build, `mbus_ffi.dll` should appear in
   `bin\Debug\net8.0\` (or `bin\Release\net8.0\`). If it is missing, re-run
   Cargo and rebuild the .NET project.

4. **Architecture mismatch (x86 vs x64).**  
   Rust defaults to the host architecture (usually x64). If your VS project
   targets x86 (or "Any CPU" with "Prefer 32-bit" enabled), keep both sides
   on x64. Disable "Prefer 32-bit" in Project Properties → Build.

5. **Missing Visual C++ Redistributable (Windows).**  
   The Rust-compiled DLL links the MSVC runtime. If the target machine lacks
   the Visual C++ 2019/2022 Redistributable, install
   [vc_redist.x64.exe](https://aka.ms/vs/17/release/vc_redist.x64.exe).

## Visual Studio 2022 Setup

1. **Build the native library** (required before first open and after any Rust change):

   ```powershell
   cargo build -p mbus-ffi --features dotnet,full
   ```

2. **Open the solution**: `File → Open → Project/Solution →`
   `<repo root>\mbus-ffi\dotnet\ModbusRs.sln`

3. **Select the configuration** (Debug or Release) — must match the Cargo build.

4. **Build the solution** (`Ctrl+Shift+B`).  
   MSBuild copies `mbus_ffi.dll` from `target\debug\` (or `target\release\`)
   into each project's output folder automatically.

5. **Run or debug** (`F5`).

> **Tip:** Add a Pre-Build Event to keep the native library in sync:  
> **Project Properties → Build Events → Pre-build event command line**:
> ```
> cargo build -p mbus-ffi --features dotnet,full
> ```
> For Release builds add `--release` to the command.

## Testing

```bash
# Build both Rust artefacts first.
cargo build -p mbus-ffi --features dotnet,full
cargo build -p mbus-ffi --example dotnet_test_server --features dotnet,full

# Run the C# test suite.
dotnet test mbus-ffi/dotnet/ModbusRs.Tests/ModbusRs.Tests.csproj
```

The test project's `.csproj` contains the same native-copy MSBuild items as the
example projects, so `dotnet test` works without any manual DLL placement.

The Rust integration test that exercises the same FFI surface lives at
[`mbus-ffi/tests/dotnet_tcp_client.rs`](../tests/dotnet_tcp_client.rs)
and runs as part of `cargo test -p mbus-ffi --features dotnet`.

## Quick start

```csharp
using ModbusRs;

using var client = new ModbusTcpClient("192.168.1.10", 502);
client.SetRequestTimeout(TimeSpan.FromSeconds(2));

await client.ConnectAsync();

// Read holding registers (FC03)
ushort[] regs = await client.ReadHoldingRegistersAsync(unitId: 1, address: 0, quantity: 4);

// Write a single register (FC06)
await client.WriteSingleRegisterAsync(unitId: 1, address: 5, value: 0xBEEF);

// Write multiple registers (FC10)
await client.WriteMultipleRegistersAsync(unitId: 1, address: 8, new ushort[] { 1, 2, 3 });

// Read coils (FC01)
bool[] coils = await client.ReadCoilsAsync(unitId: 1, address: 0, quantity: 8);

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

📖 **[Full .NET Binding Documentation →](../../documentation/dotnet_bindings.md)**

