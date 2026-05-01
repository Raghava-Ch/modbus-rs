//go:build cgo

package cgo

/*
#include <stdint.h>
#include <string.h>
*/
import "C"
import (
	"unsafe"
)

// Modbus exception codes used as the positive return value from a
// trampoline to instruct the native server to send back a protocol
// exception rather than a successful response.
const (
	exIllegalFunction      = 0x01
	exIllegalDataAddress   = 0x02
	exIllegalDataValue     = 0x03
	exServerDeviceFailure  = 0x04
)

// recoverCallbacks fetches the Go-side ServerCallbacks instance from
// the opaque ctx pointer that the server passes to every callback. ctx
// is a [runtime/cgo.Handle] packed into a uintptr.
func recoverCallbacks(ctx unsafe.Pointer) ServerCallbacks {
	if ctx == nil {
		return nil
	}
	h := voidPtrToHandle(ctx)
	v := h.Value()
	cb, _ := v.(ServerCallbacks)
	return cb
}

// errorToExceptionCode translates a Go error into the correct return
// value expected by the native server.
func errorToExceptionCode(err error) C.int32_t {
	if err == nil {
		return 0
	}
	if se, ok := err.(ServerError); ok {
		return C.int32_t(se.ExceptionCode())
	}
	return exServerDeviceFailure
}

func packBitsInto(dst []byte, bits []bool) {
	for i, b := range bits {
		if b {
			dst[i/8] |= 1 << uint(i%8)
		}
	}
}

func unpackBitsFrom(packed []byte, n int) []bool {
	out := make([]bool, n)
	for i := 0; i < n; i++ {
		if i/8 >= len(packed) {
			break
		}
		out[i] = (packed[i/8] & (1 << uint(i%8))) != 0
	}
	return out
}

// ── //export trampolines ─────────────────────────────────────────────────────
//
// Each trampoline:
//   1. Recovers the ServerCallbacks instance from `ctx`.
//   2. Calls the typed Go method.
//   3. Marshals the result (or error → exception code) back into the
//      out-buffers.
//
// Trampolines run on a Tokio worker thread (Rust-side). cgo handles the
// thread crossing transparently, but Go code invoked here MUST NOT block
// on the same Tokio thread — fan out to your own goroutines if you need
// to do heavy work.

//export goReadCoilsTrampoline
func goReadCoilsTrampoline(ctx unsafe.Pointer, addr, count C.uint16_t, outBuf *C.uint8_t, outByteCount *C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	bits, err := cb.ReadCoils(uint16(addr), uint16(count))
	if err != nil {
		return errorToExceptionCode(err)
	}
	byteCount := (len(bits) + 7) / 8
	dst := unsafe.Slice((*byte)(unsafe.Pointer(outBuf)), byteCount)
	for i := range dst {
		dst[i] = 0
	}
	packBitsInto(dst, bits)
	*outByteCount = C.uint16_t(byteCount)
	return 0
}

//export goWriteSingleCoilTrampoline
func goWriteSingleCoilTrampoline(ctx unsafe.Pointer, addr C.uint16_t, value C.uint8_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	return errorToExceptionCode(cb.WriteSingleCoil(uint16(addr), value != 0))
}

//export goWriteMultipleCoilsTrampoline
func goWriteMultipleCoilsTrampoline(ctx unsafe.Pointer, addr C.uint16_t, packed *C.uint8_t, byteCount, coilCount C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	src := unsafe.Slice((*byte)(unsafe.Pointer(packed)), int(byteCount))
	bits := unpackBitsFrom(src, int(coilCount))
	return errorToExceptionCode(cb.WriteMultipleCoils(uint16(addr), bits))
}

//export goReadDiscreteInputsTrampoline
func goReadDiscreteInputsTrampoline(ctx unsafe.Pointer, addr, count C.uint16_t, outBuf *C.uint8_t, outByteCount *C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	bits, err := cb.ReadDiscreteInputs(uint16(addr), uint16(count))
	if err != nil {
		return errorToExceptionCode(err)
	}
	byteCount := (len(bits) + 7) / 8
	dst := unsafe.Slice((*byte)(unsafe.Pointer(outBuf)), byteCount)
	for i := range dst {
		dst[i] = 0
	}
	packBitsInto(dst, bits)
	*outByteCount = C.uint16_t(byteCount)
	return 0
}

//export goReadHoldingRegistersTrampoline
func goReadHoldingRegistersTrampoline(ctx unsafe.Pointer, addr, count C.uint16_t, outBuf *C.uint16_t, outCount *C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	values, err := cb.ReadHoldingRegisters(uint16(addr), uint16(count))
	if err != nil {
		return errorToExceptionCode(err)
	}
	dst := unsafe.Slice((*uint16)(unsafe.Pointer(outBuf)), len(values))
	copy(dst, values)
	*outCount = C.uint16_t(len(values))
	return 0
}

//export goReadInputRegistersTrampoline
func goReadInputRegistersTrampoline(ctx unsafe.Pointer, addr, count C.uint16_t, outBuf *C.uint16_t, outCount *C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	values, err := cb.ReadInputRegisters(uint16(addr), uint16(count))
	if err != nil {
		return errorToExceptionCode(err)
	}
	dst := unsafe.Slice((*uint16)(unsafe.Pointer(outBuf)), len(values))
	copy(dst, values)
	*outCount = C.uint16_t(len(values))
	return 0
}

//export goWriteSingleRegisterTrampoline
func goWriteSingleRegisterTrampoline(ctx unsafe.Pointer, addr, value C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	return errorToExceptionCode(cb.WriteSingleRegister(uint16(addr), uint16(value)))
}

//export goWriteMultipleRegistersTrampoline
func goWriteMultipleRegistersTrampoline(ctx unsafe.Pointer, addr C.uint16_t, valuesBE *C.uint8_t, count C.uint16_t) C.int32_t {
	cb := recoverCallbacks(ctx)
	if cb == nil {
		return exServerDeviceFailure
	}
	// `valuesBE` is `count` u16 values in big-endian byte order, exactly
	// `2*count` bytes long.
	raw := unsafe.Slice((*byte)(unsafe.Pointer(valuesBE)), int(count)*2)
	values := make([]uint16, count)
	for i := 0; i < int(count); i++ {
		values[i] = uint16(raw[i*2])<<8 | uint16(raw[i*2+1])
	}
	return errorToExceptionCode(cb.WriteMultipleRegisters(uint16(addr), values))
}
