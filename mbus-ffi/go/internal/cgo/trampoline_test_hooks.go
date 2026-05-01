//go:build cgo

package cgo

/*
#include <stdint.h>
#include <stdlib.h>

static void *mbus_go_test_handle_to_ctx(uintptr_t h) {
    return (void *)h;
}
*/
import "C"

import (
	"runtime/cgo"
	"unsafe"
)

func withTestTrampolineContext(cb ServerCallbacks, fn func(unsafe.Pointer) int32) int32 {
	h := cgo.NewHandle(cb)
	defer h.Delete()
	return fn(C.mbus_go_test_handle_to_ctx(C.uintptr_t(h)))
}

func testReadHoldingRegistersTrampoline(cb ServerCallbacks, addr, count uint16, out []uint16) (int32, uint16) {
	var written C.uint16_t
	rc := withTestTrampolineContext(cb, func(ctx unsafe.Pointer) int32 {
		return int32(goReadHoldingRegistersTrampoline(ctx, C.uint16_t(addr), C.uint16_t(count), (*C.uint16_t)(unsafe.Pointer(&out[0])), &written))
	})
	return rc, uint16(written)
}

func testReadCoilsTrampoline(cb ServerCallbacks, addr, count uint16, out []byte) (int32, uint16) {
	var written C.uint16_t
	rc := withTestTrampolineContext(cb, func(ctx unsafe.Pointer) int32 {
		return int32(goReadCoilsTrampoline(ctx, C.uint16_t(addr), C.uint16_t(count), (*C.uint8_t)(unsafe.Pointer(&out[0])), &written))
	})
	return rc, uint16(written)
}

func testWriteMultipleRegistersTrampoline(cb ServerCallbacks, addr uint16, raw []byte, count uint16) int32 {
	return withTestTrampolineContext(cb, func(ctx unsafe.Pointer) int32 {
		return int32(goWriteMultipleRegistersTrampoline(ctx, C.uint16_t(addr), (*C.uint8_t)(unsafe.Pointer(&raw[0])), C.uint16_t(count)))
	})
}

func testWriteMultipleCoilsTrampoline(cb ServerCallbacks, addr uint16, raw []byte, byteCount, coilCount uint16) int32 {
	return withTestTrampolineContext(cb, func(ctx unsafe.Pointer) int32 {
		return int32(goWriteMultipleCoilsTrampoline(ctx, C.uint16_t(addr), (*C.uint8_t)(unsafe.Pointer(&raw[0])), C.uint16_t(byteCount), C.uint16_t(coilCount)))
	})
}

func testWriteSingleRegisterTrampoline(cb ServerCallbacks, addr, value uint16) int32 {
	return withTestTrampolineContext(cb, func(ctx unsafe.Pointer) int32 {
		return int32(goWriteSingleRegisterTrampoline(ctx, C.uint16_t(addr), C.uint16_t(value)))
	})
}
