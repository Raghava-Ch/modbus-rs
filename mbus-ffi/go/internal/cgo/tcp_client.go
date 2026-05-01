//go:build cgo

package cgo

/*
#include "modbus_rs_go.h"
#include <stdlib.h>
*/
import "C"
import (
	"unsafe"
)

// ── TCP client ───────────────────────────────────────────────────────────────

// TcpClient is an opaque handle to a native async Modbus TCP client.
type TcpClient struct{ raw *C.MbusGoTcpClient }

// TcpClientNew constructs a new TCP client targeting host:port. Returns nil
// on configuration failure (invalid host, runtime init error, …).
func TcpClientNew(host string, port uint16) *TcpClient {
	chost := C.CString(host)
	defer C.free(unsafe.Pointer(chost))
	h := C.mbus_go_tcp_client_new(chost, C.uint16_t(port))
	if h == nil {
		return nil
	}
	return &TcpClient{raw: h}
}

// TcpClientFree releases the native handle. Safe to call with nil.
func TcpClientFree(c *TcpClient) {
	if c != nil {
		C.mbus_go_tcp_client_free(c.raw)
	}
}

// TcpClientConnect opens the TCP transport. Blocks the calling thread.
func TcpClientConnect(c *TcpClient) Status {
	return Status(C.mbus_go_tcp_client_connect(c.raw))
}

// TcpClientDisconnect closes the TCP transport gracefully.
func TcpClientDisconnect(c *TcpClient) Status {
	return Status(C.mbus_go_tcp_client_disconnect(c.raw))
}

// TcpClientSetRequestTimeoutMs sets the per-request timeout in
// milliseconds. Pass 0 to disable.
func TcpClientSetRequestTimeoutMs(c *TcpClient, ms uint64) Status {
	return Status(C.mbus_go_tcp_client_set_request_timeout_ms(c.raw, C.uint64_t(ms)))
}

// TcpClientHasPendingRequests returns true if requests are in flight.
func TcpClientHasPendingRequests(c *TcpClient) bool {
	return C.mbus_go_tcp_client_has_pending_requests(c.raw) != 0
}

// TcpClientReadHoldingRegisters performs FC03.
func TcpClientReadHoldingRegisters(c *TcpClient, unit uint8, addr, qty uint16) ([]uint16, Status) {
	out := make([]uint16, qty)
	var written C.uint16_t
	var outPtr *C.uint16_t
	if qty > 0 {
		outPtr = (*C.uint16_t)(unsafe.Pointer(&out[0]))
	}
	st := Status(C.mbus_go_tcp_client_read_holding_registers(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(qty),
		outPtr, C.uint16_t(qty), &written,
	))
	if st != StatusOK {
		return nil, st
	}
	return out[:int(written)], StatusOK
}

// TcpClientReadInputRegisters performs FC04.
func TcpClientReadInputRegisters(c *TcpClient, unit uint8, addr, qty uint16) ([]uint16, Status) {
	out := make([]uint16, qty)
	var written C.uint16_t
	var outPtr *C.uint16_t
	if qty > 0 {
		outPtr = (*C.uint16_t)(unsafe.Pointer(&out[0]))
	}
	st := Status(C.mbus_go_tcp_client_read_input_registers(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(qty),
		outPtr, C.uint16_t(qty), &written,
	))
	if st != StatusOK {
		return nil, st
	}
	return out[:int(written)], StatusOK
}

// TcpClientWriteSingleRegister performs FC06.
func TcpClientWriteSingleRegister(c *TcpClient, unit uint8, addr, value uint16) Status {
	return Status(C.mbus_go_tcp_client_write_single_register(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(value), nil, nil,
	))
}

// TcpClientWriteMultipleRegisters performs FC10.
func TcpClientWriteMultipleRegisters(c *TcpClient, unit uint8, addr uint16, values []uint16) Status {
	if len(values) == 0 {
		return StatusInvalidQuantity
	}
	return Status(C.mbus_go_tcp_client_write_multiple_registers(
		c.raw, C.uint8_t(unit), C.uint16_t(addr),
		(*C.uint16_t)(unsafe.Pointer(&values[0])), C.uint16_t(len(values)),
		nil, nil,
	))
}

// TcpClientMaskWriteRegister performs FC22.
func TcpClientMaskWriteRegister(c *TcpClient, unit uint8, addr, andMask, orMask uint16) Status {
	return Status(C.mbus_go_tcp_client_mask_write_register(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(andMask), C.uint16_t(orMask),
	))
}

// TcpClientReadCoils performs FC01. Returns a packed-byte buffer (caller
// unpacks into bool slice).
func TcpClientReadCoils(c *TcpClient, unit uint8, addr, qty uint16) ([]byte, Status) {
	bufLen := (qty + 7) / 8
	out := make([]byte, bufLen)
	var written C.uint16_t
	var outPtr *C.uint8_t
	if bufLen > 0 {
		outPtr = (*C.uint8_t)(unsafe.Pointer(&out[0]))
	}
	st := Status(C.mbus_go_tcp_client_read_coils(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(qty),
		outPtr, C.uint16_t(bufLen), &written,
	))
	if st != StatusOK {
		return nil, st
	}
	return out[:int(written)], StatusOK
}

// TcpClientReadDiscreteInputs performs FC02. Returns a packed-byte buffer.
func TcpClientReadDiscreteInputs(c *TcpClient, unit uint8, addr, qty uint16) ([]byte, Status) {
	bufLen := (qty + 7) / 8
	out := make([]byte, bufLen)
	var written C.uint16_t
	var outPtr *C.uint8_t
	if bufLen > 0 {
		outPtr = (*C.uint8_t)(unsafe.Pointer(&out[0]))
	}
	st := Status(C.mbus_go_tcp_client_read_discrete_inputs(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(qty),
		outPtr, C.uint16_t(bufLen), &written,
	))
	if st != StatusOK {
		return nil, st
	}
	return out[:int(written)], StatusOK
}

// TcpClientWriteSingleCoil performs FC05.
func TcpClientWriteSingleCoil(c *TcpClient, unit uint8, addr uint16, value bool) Status {
	v := C.uint8_t(0)
	if value {
		v = 1
	}
	return Status(C.mbus_go_tcp_client_write_single_coil(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), v, nil, nil,
	))
}

// TcpClientWriteMultipleCoils performs FC0F. `packed` is the packed-byte
// representation of `count` coil bits (LSB-first per Modbus spec).
func TcpClientWriteMultipleCoils(c *TcpClient, unit uint8, addr uint16, packed []byte, count uint16) Status {
	if count == 0 {
		return StatusInvalidQuantity
	}
	if len(packed) == 0 {
		return StatusInvalidByteCount
	}
	return Status(C.mbus_go_tcp_client_write_multiple_coils(
		c.raw, C.uint8_t(unit), C.uint16_t(addr),
		(*C.uint8_t)(unsafe.Pointer(&packed[0])), C.uint16_t(len(packed)), C.uint16_t(count),
		nil, nil,
	))
}
