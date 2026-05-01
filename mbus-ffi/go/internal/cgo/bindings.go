//go:build cgo

// Package cgo links the Go module against the modbus-rs `mbus_ffi`
// cdylib via static linking by default.
//
// To switch to dynamic linking instead (loading `libmbus_ffi.so` /
// `mbus_ffi.dll` from the system loader path at run time), build with
// the `modbus_dynamic` build tag:
//
//	go build -tags modbus_dynamic ./...

package cgo

/*
#cgo CFLAGS: -I${SRCDIR}/include

// ── Default: static linking ─────────────────────────────────────────────
// We link against libmbus_ffi.a vendored in `internal/cgo/lib/<os>_<arch>/`.
// The build script `scripts/build_native.sh` copies the freshly-built archive
// into that directory on every release build.
//
// On Windows the archive is built with --target x86_64-pc-windows-gnu so that
// the resulting object files use the MinGW ABI (no MSVC __chkstk references)
// and are compatible with the MinGW gcc/ld toolchain that cgo uses.

#cgo !modbus_dynamic,linux,amd64   LDFLAGS: -L${SRCDIR}/lib/linux_amd64 -lmbus_ffi -ludev -ldl -lm -lpthread
#cgo !modbus_dynamic,linux,arm64   LDFLAGS: -L${SRCDIR}/lib/linux_arm64 -lmbus_ffi -ludev -ldl -lm -lpthread
#cgo !modbus_dynamic,darwin,amd64  LDFLAGS: -L${SRCDIR}/lib/darwin_amd64 -lmbus_ffi -framework CoreFoundation -framework IOKit -framework Security
#cgo !modbus_dynamic,darwin,arm64  LDFLAGS: -L${SRCDIR}/lib/darwin_arm64 -lmbus_ffi -framework CoreFoundation -framework IOKit -framework Security
#cgo !modbus_dynamic,windows,amd64 LDFLAGS: -L${SRCDIR}/lib/windows_amd64 -lmbus_ffi -lws2_32 -lbcrypt -lntdll -luserenv -lsetupapi

// ── Optional: dynamic linking via `-tags modbus_dynamic` ────────────────
#cgo modbus_dynamic LDFLAGS: -lmbus_ffi

#include "modbus_rs_go.h"
*/
import "C"
