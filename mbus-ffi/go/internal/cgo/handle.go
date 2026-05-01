//go:build cgo

// Package cgo: handle helpers exposed to higher-level packages that
// need to round-trip an opaque pointer through a uintptr (for atomic
// storage, finalizer handoff, etc.).

package cgo

import "unsafe"

// HandleToUintptr converts an opaque native pointer into a uintptr for
// storage in atomic / non-pointer fields.
//
// The Go GC never tracks `unsafe.Pointer`-derived `uintptr` values, so
// the caller MUST keep the underlying allocation alive by other means
// (e.g. by holding the original `*Client` reference until `Close()` is
// called).
func HandleToUintptr(p unsafe.Pointer) uintptr {
	return uintptr(p)
}

// HandleFromUintptr is the inverse of [HandleToUintptr].
func HandleFromUintptr(p uintptr) unsafe.Pointer {
	return unsafe.Pointer(p) //nolint:govet
}

// TcpClientToHandle / TcpClientFromHandle let higher-level packages
// store a *TcpClient as a uintptr without importing "C" directly.
func TcpClientToHandle(p *TcpClient) uintptr {
	return uintptr(unsafe.Pointer(p))
}

// TcpClientFromHandle is the inverse of [TcpClientToHandle].
func TcpClientFromHandle(p uintptr) *TcpClient {
	if p == 0 {
		return nil
	}
	return (*TcpClient)(unsafe.Pointer(p)) //nolint:govet
}

