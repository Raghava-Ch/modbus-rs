# Go Bindings

The Go bindings expose `modbus-rs` through a cgo module at:

```text
github.com/Raghava-Ch/modbus-rs/mbus-ffi/go
```

They follow the same native binding architecture as the .NET binding: a thin
`extern "C"` ABI over the existing async Rust crates, one shared Tokio runtime,
opaque heap handles, and a vtable for server callbacks. The Go surface is
idiomatic Go: `context.Context`, `error`, package-per-role layout, exported Go
values instead of C values, and `Close` methods that satisfy `io.Closer`.

No Modbus protocol logic is reimplemented in Go. Requests flow through:

```text
Go public API → internal/cgo wrapper → mbus_go_* C ABI → async Rust crates
```

## Status

Implemented packages:

| Package | Purpose |
|---|---|
| `modbus` | Shared status codes, function codes, exception codes, serial mode, errors |
| `client/tcp` | Async Modbus TCP client |
| `client/serial` | Async Modbus serial RTU/ASCII client |
| `server/tcp` | Async Modbus TCP server backed by a Go `Handler` |
| `gateway/tcp` | Async Modbus TCP gateway with a Go `Router` builder |

Planned follow-ups:

- Serial server package.
- True Rust-side cancellation-token plumbing for `context.Context` cancellation.
  Current Go methods return early when the context is cancelled, but the native
  request future is allowed to finish in the background.

## Build prerequisites

The bindings require cgo and a native `libmbus_ffi` static archive for the host
platform.

### Linux

Install `libudev` development headers because serial support links through the
Rust `serialport` / `libudev` stack:

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends libudev-dev
```

Then build and vendor the host archive:

```sh
./mbus-ffi/go/scripts/build_native.sh
cd mbus-ffi/go
go test ./...
```

### macOS

Install Rust and Go. Xcode command-line tools are required for cgo:

```sh
xcode-select --install
./mbus-ffi/go/scripts/build_native.sh
cd mbus-ffi/go
go test ./...
```

### Windows

Rust, Go, and a MinGW-w64 toolchain (for cgo) are required. The native library
**must be built with the `x86_64-pc-windows-gnu` Rust target**, not the default
MSVC target. The MSVC target emits calls to `__chkstk` (a MSVC CRT symbol) that
MinGW's `ld.exe` cannot resolve, causing link errors.

```sh
rustup target add x86_64-pc-windows-gnu
./mbus-ffi/go/scripts/build_native.sh
```

The script detects Windows and automatically uses the GNU target, producing
`libmbus_ffi.a` (GNU-format archive) in:

```text
mbus-ffi/go/internal/cgo/lib/windows_amd64/libmbus_ffi.a
```

## Feature flags

Build the native library with:

```sh
cargo build --release -p mbus-ffi --features go,full
```

`go` enables:

- `coils`
- `registers`
- `discrete-inputs`
- `fifo`
- `file-record`
- `diagnostics`
- serial RTU/ASCII client transports
- async TCP server and async TCP gateway support

Use `go-traffic` if you also need the async traffic-notifier features:

```sh
cargo build --release -p mbus-ffi --features go,go-traffic,full
```

## Linking model

By default Go links statically against a platform-specific archive:

| Platform | Archive |
|---|---|
| Linux amd64 | `mbus-ffi/go/internal/cgo/lib/linux_amd64/libmbus_ffi.a` |
| Linux arm64 | `mbus-ffi/go/internal/cgo/lib/linux_arm64/libmbus_ffi.a` |
| macOS amd64 | `mbus-ffi/go/internal/cgo/lib/darwin_amd64/libmbus_ffi.a` |
| macOS arm64 | `mbus-ffi/go/internal/cgo/lib/darwin_arm64/libmbus_ffi.a` |
| Windows amd64 | `mbus-ffi/go/internal/cgo/lib/windows_amd64/libmbus_ffi.lib` |

For dynamic linking, build with:

```sh
go build -tags modbus_dynamic ./...
```

and ensure `libmbus_ffi` is available to the platform dynamic loader.

## Package overview

### Shared `modbus` package

Import path:

```go
import "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
```

The package defines:

- `Status`: mirrors Rust `MbusStatusCode`.
- `FunctionCode`: standard Modbus function codes.
- `ExceptionCode`: standard Modbus protocol exception codes.
- `SerialMode`: `SerialRTU` or `SerialASCII`.
- `Error`: structured Go error wrapping a native status.
- Sentinel errors for `errors.Is`: `ErrTimeout`, `ErrNotConnected`,
  `ErrClosed`, `ErrInvalidArgument`, `ErrConnectionLost`, `ErrIO`.

Example:

```go
err := modbus.FromStatus("Connect", modbus.StatusTimeout)
if errors.Is(err, modbus.ErrTimeout) {
    // retry or surface timeout to the caller
}
```

### TCP client

Import path:

```go
import "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
```

Lifecycle:

1. `tcp.NewClient(host, port, opts...)`
2. `Connect(ctx)`
3. Call request methods.
4. `Disconnect(ctx)` if you want to reconnect later, or `Close()` to free the
   native handle.

```go
ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
defer cancel()

c, err := tcp.NewClient("127.0.0.1", 1502, tcp.WithTimeout(2*time.Second))
if err != nil {
    return err
}
defer c.Close()

if err := c.Connect(ctx); err != nil {
    return err
}

regs, err := c.ReadHoldingRegisters(ctx, 1, 0, 4)
if err != nil {
    return err
}
_ = regs
```

Supported methods currently exposed by the Go wrapper:

- `ReadCoils` (FC01)
- `ReadDiscreteInputs` (FC02)
- `ReadHoldingRegisters` (FC03)
- `ReadInputRegisters` (FC04)
- `WriteSingleCoil` (FC05)
- `WriteSingleRegister` (FC06)
- `WriteMultipleCoils` (FC0F)
- `WriteMultipleRegisters` (FC10)
- `MaskWriteRegister` (FC16)

The Rust FFI exports additional function codes (`fifo`, `file-record`,
`diagnostics`) and the Go public wrappers can be extended without changing the
ABI prefix or module layout.

### Serial client

Import path:

```go
import "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/serial"
```

```go
c, err := serial.NewClient("/dev/ttyUSB0", 9600,
    serial.WithMode(modbus.SerialRTU),
    serial.WithDataBits(8),
    serial.WithParity(0), // 0=None, 1=Odd, 2=Even
    serial.WithStopBits(1),
    serial.WithTimeout(2*time.Second),
)
if err != nil {
    return err
}
defer c.Close()

if err := c.Connect(ctx); err != nil {
    return err
}

value, err := c.ReadHoldingRegisters(ctx, 1, 0, 1)
_ = value
_ = err
```

The serial package currently exposes FC03 and FC06. Additional client methods
can be added as thin wrappers around the already-exported `mbus_go_serial_*`
FFI symbols.

### TCP server

Import path:

```go
import servertcp "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/server/tcp"
```

Implement `Handler`, usually by embedding `BaseHandler` and overriding the
function codes your device supports. Unimplemented methods return Modbus
`IllegalFunction`.

```go
type device struct {
    servertcp.BaseHandler
    holding [128]uint16
}

func (d *device) ReadHoldingRegisters(ctx context.Context, unit uint8, addr, count uint16) ([]uint16, error) {
    if int(addr)+int(count) > len(d.holding) {
        return nil, servertcp.IllegalDataAddress()
    }
    out := make([]uint16, count)
    copy(out, d.holding[addr:addr+count])
    return out, nil
}

srv, err := servertcp.NewServer("0.0.0.0:1502", &device{})
if err != nil {
    return err
}
defer srv.Close()
return srv.Serve(ctx)
```

Handler methods may return:

- `nil`: successful response.
- `IllegalFunction()`, `IllegalDataAddress()`, `IllegalDataValue()`,
  `ServerDeviceFailure()`, or `WithException(code)`: protocol exception
  response.
- any other error: mapped to Server Device Failure.

Callbacks are invoked from Rust/Tokio worker threads through cgo. This is
supported by the Go runtime, but handlers should avoid long blocking work on the
callback thread. If necessary, hand work to an application goroutine through a
channel and wait with a bounded timeout.

### TCP gateway

Import path:

```go
import gatewaytcp "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/gateway/tcp"
```

```go
router := gatewaytcp.NewRouter().
    AddUnit(1, 0).
    AddRange(2, 10, 1)

gw, err := gatewaytcp.NewGateway("0.0.0.0:5020", []gatewaytcp.Downstream{
    {Host: "127.0.0.1", Port: 1502},
    {Host: "127.0.0.1", Port: 1503},
}, router)
if err != nil {
    return err
}
defer gw.Close()
return gw.Serve(ctx)
```

Channel indices in the router refer to the order of the downstream slice passed
to `NewGateway`.

## Threading and context behavior

Every Go request method calls into an FFI function that blocks on the shared
Tokio runtime. The cgo call releases the current Go thread back to the scheduler,
so other goroutines can continue to run normally.

`context.Context` is checked before each call and while waiting for completion.
If the context is cancelled first, the Go method returns `ctx.Err()`. The native
operation is not aborted yet; it completes in the background and its result is
discarded. A future cancellation-token ABI will propagate cancellation into the
Tokio future.

## Testing the bindings

Recommended local checks:

```sh
sudo apt-get install -y --no-install-recommends libudev-dev  # Linux only
./mbus-ffi/go/scripts/build_native.sh

cd mbus-ffi/go
gofmt -d .
go vet ./...
go test -race -count=1 ./...
go test -tags integration -count=1 ./client/tcp/...

cd ../..
cargo test -p mbus-ffi --features go,full --lib go::
```

The tests include:

- public API lifecycle tests for clients, server, and gateway;
- binding-layer tests for cgo status mapping and callback trampoline
  marshalling;
- Go server ↔ Go client round trips;
- Rust example server ↔ Go TCP client integration round trip.

## Release tagging

The Go module uses a path-prefixed tag:

```sh
git tag mbus-ffi/go/v0.8.0
git push origin mbus-ffi/go/v0.8.0
```

Consumers can then use:

```sh
go get github.com/Raghava-Ch/modbus-rs/mbus-ffi/go@v0.8.0
```
