package modbus_test

import (
	"errors"
	"strings"
	"testing"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

func TestFromStatusMapsSentinelErrors(t *testing.T) {
	cases := []struct {
		name   string
		status modbus.Status
		want   error
	}{
		{"ok", modbus.StatusOK, nil},
		{"timeout", modbus.StatusTimeout, modbus.ErrTimeout},
		{"closed", modbus.StatusConnectionClosed, modbus.ErrNotConnected},
		{"lost", modbus.StatusConnectionLost, modbus.ErrConnectionLost},
		{"invalid", modbus.StatusInvalidQuantity, modbus.ErrInvalidArgument},
		{"io", modbus.StatusIoError, modbus.ErrIO},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := modbus.FromStatus("op", tc.status)
			if tc.want == nil {
				if err != nil {
					t.Fatalf("FromStatus returned %v, want nil", err)
				}
				return
			}
			if !errors.Is(err, tc.want) {
				t.Fatalf("FromStatus(%v) = %v, want errors.Is(..., %v)", tc.status, err, tc.want)
			}
			var mErr *modbus.Error
			if !errors.As(err, &mErr) {
				t.Fatalf("FromStatus returned %T, want *modbus.Error", err)
			}
			if mErr.Status != tc.status || mErr.Op != "op" {
				t.Fatalf("unexpected structured error: %+v", mErr)
			}
		})
	}
}

func TestErrorStringIncludesOperationAndStatus(t *testing.T) {
	err := modbus.FromStatus("ReadHoldingRegisters", modbus.StatusTimeout)
	got := err.Error()
	if !strings.Contains(got, "ReadHoldingRegisters") || !strings.Contains(got, "timeout") {
		t.Fatalf("Error() = %q, want operation and status", got)
	}
}

func TestExceptionCodeString(t *testing.T) {
	if got := modbus.ExIllegalDataAddress.String(); got != "illegal data address" {
		t.Fatalf("ExceptionCode.String() = %q", got)
	}
	if got := modbus.ExceptionCode(0xEE).String(); got != "unknown(0xee)" {
		t.Fatalf("unknown ExceptionCode.String() = %q", got)
	}
}
