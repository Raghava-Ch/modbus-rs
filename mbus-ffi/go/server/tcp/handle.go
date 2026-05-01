package tcp

import "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"

func toUintptr(p *cgo.TcpServer) uintptr   { return cgo.TcpServerToHandle(p) }
func fromUintptr(p uintptr) *cgo.TcpServer { return cgo.TcpServerFromHandle(p) }
