package tcp

import "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"

// toUintptr / fromUintptr proxy through the cgo helpers so this
// package never has to import "C" directly.
func toUintptr(p *cgo.TcpClient) uintptr   { return cgo.TcpClientToHandle(p) }
func fromUintptr(p uintptr) *cgo.TcpClient { return cgo.TcpClientFromHandle(p) }
