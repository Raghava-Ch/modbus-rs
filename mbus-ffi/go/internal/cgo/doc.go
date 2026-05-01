// Package cgo holds the low-level cgo declarations for the Go bindings
// to the modbus-rs `mbus_ffi` cdylib.
//
// This package is intentionally internal — its API is unstable and not
// part of the public Go module surface. All public access goes through
// the `client/`, `server/` and `gateway/` packages.
package cgo
