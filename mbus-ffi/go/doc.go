// Package modbusrs is the umbrella documentation entry point for the
// idiomatic Go bindings to the modbus-rs Modbus stack.
//
// The bindings are split across role-focused sub-packages:
//
//   - [github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus]            shared types and errors
//   - [github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp]        async Modbus TCP client
//   - [github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/serial]     async Modbus RTU/ASCII client
//   - [github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/server/tcp]        async Modbus TCP server (Handler interface)
//   - [github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/gateway/tcp]       async Modbus TCP gateway with Router
//
// # Architecture
//
// All sub-packages are thin idiomatic wrappers around the modbus-rs
// `mbus_ffi` cdylib. The native library is statically linked into the
// Go binary by default, so `go build` produces a single self-contained
// executable for the host platform. The shared Tokio runtime that drives
// every async request is created on first use inside the native library
// and reused for the lifetime of the process.
//
// # Build prerequisites
//
// Before the first `go build`, you must build the native library and
// vendor it into this module:
//
//	cargo build --release -p mbus-ffi --features go,full
//	./mbus-ffi/go/scripts/build_native.sh   # copies libmbus_ffi.a + header
//
// The build script `mbus-ffi/build.rs` automatically refreshes
// `internal/cgo/include/modbus_rs_go.h` on every Cargo build of the
// `go` feature, so the vendored header always matches the FFI surface.
//
// # Concurrency model
//
// All client/server/gateway types are safe to use from multiple
// goroutines concurrently. Each request method takes a
// [context.Context] for deadline enforcement; cancellation propagation
// into in-flight native operations is on the roadmap.
//
// Server callbacks (the methods on a [server/tcp.Handler]) are invoked
// on Tokio worker threads via cgo. They may freely call Go runtime
// services but should avoid blocking arbitrarily — fan out to your own
// goroutines if you need to do heavy work in response to a request.
package modbusrs
