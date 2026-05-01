//go:build cgo

package cgo

/*
#include "modbus_rs_go.h"
#include <stdlib.h>
*/
import "C"
import "unsafe"

// SerialClient is an opaque handle to a native async serial Modbus client.
type SerialClient struct{ raw *C.MbusGoSerialClient }

// SerialClientNewRTU constructs a new RTU client on the given port.
func SerialClientNewRTU(port string, baud uint32, dataBits, parity, stopBits uint8, responseTimeoutMs uint32) *SerialClient {
	cport := C.CString(port)
	defer C.free(unsafe.Pointer(cport))
	h := C.mbus_go_serial_client_new_rtu(
		cport, C.uint32_t(baud), C.uint8_t(dataBits), C.uint8_t(parity), C.uint8_t(stopBits), C.uint32_t(responseTimeoutMs),
	)
	if h == nil {
		return nil
	}
	return &SerialClient{raw: h}
}

// SerialClientNewASCII constructs a new ASCII client on the given port.
func SerialClientNewASCII(port string, baud uint32, dataBits, parity, stopBits uint8, responseTimeoutMs uint32) *SerialClient {
	cport := C.CString(port)
	defer C.free(unsafe.Pointer(cport))
	h := C.mbus_go_serial_client_new_ascii(
		cport, C.uint32_t(baud), C.uint8_t(dataBits), C.uint8_t(parity), C.uint8_t(stopBits), C.uint32_t(responseTimeoutMs),
	)
	if h == nil {
		return nil
	}
	return &SerialClient{raw: h}
}

func SerialClientFree(c *SerialClient) {
	if c != nil {
		C.mbus_go_serial_client_free(c.raw)
	}
}

func SerialClientConnect(c *SerialClient) Status {
	return Status(C.mbus_go_serial_client_connect(c.raw))
}

func SerialClientDisconnect(c *SerialClient) Status {
	return Status(C.mbus_go_serial_client_disconnect(c.raw))
}

func SerialClientSetRequestTimeoutMs(c *SerialClient, ms uint64) {
	C.mbus_go_serial_client_set_request_timeout_ms(c.raw, C.uint64_t(ms))
}

func SerialClientReadHoldingRegisters(c *SerialClient, unit uint8, addr, qty uint16) ([]uint16, Status) {
	out := make([]uint16, qty)
	var written C.uint16_t
	var outPtr *C.uint16_t
	if qty > 0 {
		outPtr = (*C.uint16_t)(unsafe.Pointer(&out[0]))
	}
	st := Status(C.mbus_go_serial_client_read_holding_registers(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(qty),
		outPtr, C.uint16_t(qty), &written,
	))
	if st != StatusOK {
		return nil, st
	}
	return out[:int(written)], StatusOK
}

func SerialClientWriteSingleRegister(c *SerialClient, unit uint8, addr, value uint16) Status {
	return Status(C.mbus_go_serial_client_write_single_register(
		c.raw, C.uint8_t(unit), C.uint16_t(addr), C.uint16_t(value), nil, nil,
	))
}

// ── Gateway ─────────────────────────────────────────────────────────────────

type TcpGateway struct{ raw *C.MbusGoTcpGateway }

func TcpGatewayNew(host string, port uint16) *TcpGateway {
	chost := C.CString(host)
	defer C.free(unsafe.Pointer(chost))
	h := C.mbus_go_tcp_gateway_new(chost, C.uint16_t(port))
	if h == nil {
		return nil
	}
	return &TcpGateway{raw: h}
}

func TcpGatewayFree(g *TcpGateway) {
	if g != nil {
		C.mbus_go_tcp_gateway_free(g.raw)
	}
}

// TcpGatewayAddDownstream registers a downstream and returns its
// channel index.
func TcpGatewayAddDownstream(g *TcpGateway, host string, port uint16) uint32 {
	chost := C.CString(host)
	defer C.free(unsafe.Pointer(chost))
	return uint32(C.mbus_go_tcp_gateway_add_downstream(g.raw, chost, C.uint16_t(port)))
}

func TcpGatewayAddUnitRoute(g *TcpGateway, unit uint8, channel uint32) Status {
	return Status(C.mbus_go_tcp_gateway_add_unit_route(g.raw, C.uint8_t(unit), C.uint32_t(channel)))
}

func TcpGatewayAddRangeRoute(g *TcpGateway, unitMin, unitMax uint8, channel uint32) Status {
	return Status(C.mbus_go_tcp_gateway_add_range_route(
		g.raw, C.uint8_t(unitMin), C.uint8_t(unitMax), C.uint32_t(channel),
	))
}

func TcpGatewayStart(g *TcpGateway) Status {
	return Status(C.mbus_go_tcp_gateway_start(g.raw))
}

func TcpGatewayStop(g *TcpGateway) {
	C.mbus_go_tcp_gateway_stop(g.raw)
}
