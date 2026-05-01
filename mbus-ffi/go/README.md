# Go bindings for `modbus-rs`

[![Go Reference](https://pkg.go.dev/badge/github.com/Raghava-Ch/modbus-rs/mbus-ffi/go.svg)](https://pkg.go.dev/github.com/Raghava-Ch/modbus-rs/mbus-ffi/go)

Async Modbus TCP and serial **client**, TCP **server** with a Go `Handler`
interface, and TCP **gateway** with a `Router` builder — all built on top of the
existing async Rust crates (`mbus-client-async`, `mbus-server-async`,
`mbus-gateway`) via a thin cgo layer. No protocol code is duplicated.

## Quick start

```go
import (
    "context"
    "log"
    "time"

    "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
)

func main() {
    c, err := tcp.NewClient("127.0.0.1", 1502, tcp.WithTimeout(2*time.Second))
    if err != nil { log.Fatal(err) }
    defer c.Close()

    ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
    defer cancel()

    if err := c.Connect(ctx); err != nil { log.Fatal(err) }
    regs, err := c.ReadHoldingRegisters(ctx, /*unit=*/1, /*addr=*/0, /*count=*/4)
    if err != nil { log.Fatal(err) }
    log.Printf("regs = %v", regs)
}
```

See [`examples/`](./examples) for runnable TCP client, server, and gateway demos.

## Building

The Go module links statically against `libmbus_ffi.a`, which is produced by
the workspace-level Rust `mbus-ffi` crate. Before `go build` / `go test`,
ensure the static archive and matching header are present under
`internal/cgo/{lib,include}/`:

```sh
# From the repository root, build the native binding for the host platform:
./mbus-ffi/go/scripts/build_native.sh

# Then, from anywhere in the Go module:
go test ./...
```

To build for a specific target (e.g. cross-compile prep on macOS arm64):

```sh
cargo build --release -p mbus-ffi --features go,full --target aarch64-apple-darwin
mkdir -p mbus-ffi/go/internal/cgo/lib/darwin_arm64
cp target/aarch64-apple-darwin/release/libmbus_ffi.a \
   mbus-ffi/go/internal/cgo/lib/darwin_arm64/
```

## Cargo features

The `go` feature in `mbus-ffi/Cargo.toml` enables a sensible default of
`coils`, `registers`, `discrete-inputs`, `fifo`, `file-record`, `diagnostics`,
serial RTU/ASCII transports, and the async TCP server + gateway. To additionally
enable per-PDU traffic logging, build with `--features go,go-traffic`.

## Linking modes

By default the cgo bindings link **statically** against `libmbus_ffi.a`,
producing a single self-contained binary. To link against a shared library
instead (`libmbus_ffi.so` / `.dylib` / `.dll`), build with the
`modbus_dynamic` build tag and ensure the shared library is on the dynamic
loader path at runtime.

## Cross-platform support

The `internal/cgo/bindings.go` file carries per-platform `LDFLAGS` for:

| OS / arch | Static archive path |
|---|---|
| `linux/amd64`   | `internal/cgo/lib/linux_amd64/libmbus_ffi.a`    |
| `linux/arm64`   | `internal/cgo/lib/linux_arm64/libmbus_ffi.a`    |
| `darwin/amd64`  | `internal/cgo/lib/darwin_amd64/libmbus_ffi.a`   |
| `darwin/arm64`  | `internal/cgo/lib/darwin_arm64/libmbus_ffi.a`   |
| `windows/amd64` | `internal/cgo/lib/windows_amd64/libmbus_ffi.lib`|

cgo is required: this module does **not** support `GOOS=js` or `tinygo`.
On Windows, a working `gcc` (mingw-w64) is required at build time.

## Concurrency model

A single global multi-threaded Tokio runtime is shared across all clients,
servers, and gateways. Every request method blocks the calling Go thread on
`runtime.block_on(...)`; cgo releases the OS thread back to the Go scheduler
during the call, so `GOMAXPROCS` is not pinned. All public types are safe to
use concurrently from multiple goroutines.

Server-side `Handler` callbacks run on Tokio worker threads. They may call
arbitrary Go code, but should avoid blocking on long-running I/O — fan out to
your own goroutines via channels if needed.

## Roadmap

- Serial **server** support (Step 5 of the design rollout).
- `context.Context` cancellation token plumbed all the way to the native
  request task so cancelling a `ctx` aborts the in-flight Tokio future
  rather than just returning early on the Go side (Step 7).

## Layout

See [`doc.go`](./doc.go) for the full package map.
