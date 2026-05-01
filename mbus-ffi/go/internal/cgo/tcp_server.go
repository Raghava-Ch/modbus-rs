//go:build cgo

package cgo

/*
#include "modbus_rs_go.h"
#include <stdlib.h>
#include <string.h>
#include <stdint.h>

// Forward declarations of the //export trampolines defined in
// trampolines.go. cgo will generate the appropriate symbol stubs.
extern int32_t goReadCoilsTrampoline(void*, uint16_t, uint16_t, uint8_t*, uint16_t*);
extern int32_t goWriteSingleCoilTrampoline(void*, uint16_t, uint8_t);
extern int32_t goWriteMultipleCoilsTrampoline(void*, uint16_t, const uint8_t*, uint16_t, uint16_t);
extern int32_t goReadDiscreteInputsTrampoline(void*, uint16_t, uint16_t, uint8_t*, uint16_t*);
extern int32_t goReadHoldingRegistersTrampoline(void*, uint16_t, uint16_t, uint16_t*, uint16_t*);
extern int32_t goReadInputRegistersTrampoline(void*, uint16_t, uint16_t, uint16_t*, uint16_t*);
extern int32_t goWriteSingleRegisterTrampoline(void*, uint16_t, uint16_t);
extern int32_t goWriteMultipleRegistersTrampoline(void*, uint16_t, const uint8_t*, uint16_t);

// Helper that converts a uintptr_t (carrying a runtime/cgo.Handle) to
// the void* type expected by the vtable's `ctx` slot. Doing the cast
// in C avoids `go vet`'s `unsafeptr` warning, which trips on
// uintptr→unsafe.Pointer conversions for non-pointer-derived values
// (a cgo.Handle is just an opaque integer).
static void *mbus_go_handle_to_ctx(uintptr_t h) {
    return (void *)h;
}

// build_server_vtable is a small C shim that fills in the
// MbusGoServerVtable struct from Go-supplied function pointers and
// caller context. Doing it here (rather than from Go) avoids having to
// take the address of //export functions on the Go side, which is
// awkward because they live in the magic _cgo_export translation unit.
static void mbus_go_build_server_vtable(struct MbusGoServerVtable *vt, void *ctx) {
    memset(vt, 0, sizeof(*vt));
    vt->ctx = ctx;
    vt->read_coils                = goReadCoilsTrampoline;
    vt->write_single_coil         = goWriteSingleCoilTrampoline;
    vt->write_multiple_coils      = goWriteMultipleCoilsTrampoline;
    vt->read_discrete_inputs      = goReadDiscreteInputsTrampoline;
    vt->read_holding_registers    = goReadHoldingRegistersTrampoline;
    vt->read_input_registers      = goReadInputRegistersTrampoline;
    vt->write_single_register     = goWriteSingleRegisterTrampoline;
    vt->write_multiple_registers  = goWriteMultipleRegistersTrampoline;
}
*/
import "C"
import (
	"runtime/cgo"
	"unsafe"
)

// TcpServer is an opaque handle to a native async Modbus TCP server.
type TcpServer struct{ raw *C.MbusGoTcpServer }

// ServerCallbacks is the set of Go-language callbacks invoked by the
// native server when a Modbus request arrives. Each method must return
// either nil (success) or an `error` that satisfies [ServerError] for
// protocol-level exceptions.
type ServerCallbacks interface {
	ReadCoils(addr, count uint16) ([]bool, error)
	WriteSingleCoil(addr uint16, value bool) error
	WriteMultipleCoils(addr uint16, values []bool) error
	ReadDiscreteInputs(addr, count uint16) ([]bool, error)
	ReadHoldingRegisters(addr, count uint16) ([]uint16, error)
	ReadInputRegisters(addr, count uint16) ([]uint16, error)
	WriteSingleRegister(addr, value uint16) error
	WriteMultipleRegisters(addr uint16, values []uint16) error
}

// ServerError is implemented by errors returned from [ServerCallbacks]
// methods that wish to translate to a specific Modbus exception code.
// Errors NOT implementing this interface are reported as Server Device
// Failure (0x04).
type ServerError interface {
	error
	ExceptionCode() uint8
}

// TcpServerNew constructs a new TCP server bound to addr. `cb` is the
// Go-side callback set; this function takes ownership of `cb` until
// [TcpServerFree] is called.
func TcpServerNew(host string, port uint16, unitID uint8, cb ServerCallbacks) (*TcpServer, cgo.Handle, Status) {
	h := cgo.NewHandle(cb)

	chost := C.CString(host)
	defer C.free(unsafe.Pointer(chost))

	var vt C.struct_MbusGoServerVtable
	C.mbus_go_build_server_vtable(&vt, C.mbus_go_handle_to_ctx(C.uintptr_t(h)))

	srv := C.mbus_go_tcp_server_new(chost, C.uint16_t(port), C.uint8_t(unitID), &vt)
	if srv == nil {
		h.Delete()
		return nil, 0, StatusInvalidConfiguration
	}
	return &TcpServer{raw: srv}, h, StatusOK
}

// (No more uintptr↔unsafe.Pointer round-trips on the Go side: the
// cgo.Handle reaches the C side as a uintptr_t and is type-laundered
// to `void*` by mbus_go_handle_to_ctx in the C preamble. The reverse
// direction lives in trampolines.go.)

// TcpServerFree releases the native handle and the cgo callback handle.
func TcpServerFree(s *TcpServer, h cgo.Handle) {
	if s != nil {
		C.mbus_go_tcp_server_free(s.raw)
	}
	if h != 0 {
		h.Delete()
	}
}

// TcpServerStart spawns the listener thread and starts accepting
// connections. Returns immediately; use [TcpServerStop] to terminate.
func TcpServerStart(s *TcpServer) Status {
	return Status(C.mbus_go_tcp_server_start(s.raw))
}

// TcpServerStop signals the running server to terminate. The native
// thread will finish in-flight sessions and exit.
func TcpServerStop(s *TcpServer) {
	C.mbus_go_tcp_server_stop(s.raw)
}
