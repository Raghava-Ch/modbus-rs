package tcp

import (
	"context"
	"testing"
)

type unitCaptureHandler struct {
	BaseHandler
	lastUnit uint8
}

func (h *unitCaptureHandler) ReadHoldingRegisters(_ context.Context, unit uint8, _, count uint16) ([]uint16, error) {
	h.lastUnit = unit
	return make([]uint16, count), nil
}

func TestAdapterUsesConfiguredUnitID(t *testing.T) {
	h := &unitCaptureHandler{}
	a := newAdapter(h, 17)
	if _, err := a.ReadHoldingRegisters(0, 1); err != nil {
		t.Fatalf("ReadHoldingRegisters: %v", err)
	}
	if h.lastUnit != 17 {
		t.Fatalf("handler saw unit %d, want 17", h.lastUnit)
	}
}
