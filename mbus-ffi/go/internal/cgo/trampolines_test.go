package cgo

import (
	"errors"
	"testing"
)

type trampolineCallbacks struct {
	coils     []bool
	discretes []bool
	holding   []uint16
	input     []uint16

	writeSingleCoilAddr  uint16
	writeSingleCoilValue bool
	writeManyCoilsAddr   uint16
	writeManyCoilsValues []bool

	writeSingleRegisterAddr  uint16
	writeSingleRegisterValue uint16
	writeManyRegistersAddr   uint16
	writeManyRegistersValues []uint16

	err error
}

func (c *trampolineCallbacks) ReadCoils(uint16, uint16) ([]bool, error) {
	return append([]bool(nil), c.coils...), c.err
}
func (c *trampolineCallbacks) WriteSingleCoil(addr uint16, value bool) error {
	c.writeSingleCoilAddr = addr
	c.writeSingleCoilValue = value
	return c.err
}
func (c *trampolineCallbacks) WriteMultipleCoils(addr uint16, values []bool) error {
	c.writeManyCoilsAddr = addr
	c.writeManyCoilsValues = append([]bool(nil), values...)
	return c.err
}
func (c *trampolineCallbacks) ReadDiscreteInputs(uint16, uint16) ([]bool, error) {
	return append([]bool(nil), c.discretes...), c.err
}
func (c *trampolineCallbacks) ReadHoldingRegisters(uint16, uint16) ([]uint16, error) {
	return append([]uint16(nil), c.holding...), c.err
}
func (c *trampolineCallbacks) ReadInputRegisters(uint16, uint16) ([]uint16, error) {
	return append([]uint16(nil), c.input...), c.err
}
func (c *trampolineCallbacks) WriteSingleRegister(addr, value uint16) error {
	c.writeSingleRegisterAddr = addr
	c.writeSingleRegisterValue = value
	return c.err
}
func (c *trampolineCallbacks) WriteMultipleRegisters(addr uint16, values []uint16) error {
	c.writeManyRegistersAddr = addr
	c.writeManyRegistersValues = append([]uint16(nil), values...)
	return c.err
}

type trampolineException uint8

func (e trampolineException) Error() string        { return "exception" }
func (e trampolineException) ExceptionCode() uint8 { return uint8(e) }

func TestTrampolineReadHoldingRegistersMarshalsWords(t *testing.T) {
	buf := make([]uint16, 3)
	rc, count := testReadHoldingRegistersTrampoline(&trampolineCallbacks{holding: []uint16{0x1234, 0xABCD, 0x0001}}, 10, 3, buf)
	if rc != 0 {
		t.Fatalf("rc = %d, want 0", rc)
	}
	if count != 3 {
		t.Fatalf("count = %d, want 3", count)
	}
	want := []uint16{0x1234, 0xABCD, 0x0001}
	for i := range want {
		if buf[i] != want[i] {
			t.Fatalf("word[%d] = %#04x, want %#04x", i, buf[i], want[i])
		}
	}
}

func TestTrampolineReadCoilsPacksBits(t *testing.T) {
	buf := make([]byte, 2)
	rc, byteCount := testReadCoilsTrampoline(&trampolineCallbacks{coils: []bool{true, false, true, true, false, false, false, true, true}}, 0, 9, buf)
	if rc != 0 {
		t.Fatalf("rc = %d, want 0", rc)
	}
	if byteCount != 2 {
		t.Fatalf("byteCount = %d, want 2", byteCount)
	}
	if buf[0] != 0b10001101 || buf[1] != 0b00000001 {
		t.Fatalf("packed bits = [%08b %08b], want [10001101 00000001]", buf[0], buf[1])
	}
}

func TestTrampolineWriteMultipleRegistersDecodesBigEndianBytes(t *testing.T) {
	cb := &trampolineCallbacks{}
	raw := []byte{0x12, 0x34, 0xAB, 0xCD, 0x00, 0x01}
	if rc := testWriteMultipleRegistersTrampoline(cb, 42, raw, 3); rc != 0 {
		t.Fatalf("rc = %d, want 0", rc)
	}
	if cb.writeManyRegistersAddr != 42 {
		t.Fatalf("addr = %d, want 42", cb.writeManyRegistersAddr)
	}
	want := []uint16{0x1234, 0xABCD, 0x0001}
	for i := range want {
		if cb.writeManyRegistersValues[i] != want[i] {
			t.Fatalf("value[%d] = %#04x, want %#04x", i, cb.writeManyRegistersValues[i], want[i])
		}
	}
}

func TestTrampolineWriteMultipleCoilsUnpacksBits(t *testing.T) {
	cb := &trampolineCallbacks{}
	raw := []byte{0b10001101, 0b00000001}
	if rc := testWriteMultipleCoilsTrampoline(cb, 7, raw, 2, 9); rc != 0 {
		t.Fatalf("rc = %d, want 0", rc)
	}
	want := []bool{true, false, true, true, false, false, false, true, true}
	for i := range want {
		if cb.writeManyCoilsValues[i] != want[i] {
			t.Fatalf("coil[%d] = %v, want %v", i, cb.writeManyCoilsValues[i], want[i])
		}
	}
}

func TestTrampolineErrorMapping(t *testing.T) {
	if rc := testWriteSingleRegisterTrampoline(&trampolineCallbacks{err: trampolineException(2)}, 1, 2); rc != 2 {
		t.Fatalf("exception rc = %d, want 2", rc)
	}
	if rc := testWriteSingleRegisterTrampoline(&trampolineCallbacks{err: errors.New("boom")}, 1, 2); rc != exServerDeviceFailure {
		t.Fatalf("generic rc = %d, want %d", rc, exServerDeviceFailure)
	}
}
