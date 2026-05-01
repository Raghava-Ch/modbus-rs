package serial_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/serial"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// TestNewClientNonExistentPort verifies the constructor accepts the
// configuration but Connect fails cleanly against a port that does not
// exist (no panic, no leak).
func TestNewClientNonExistentPort(t *testing.T) {
	c, err := serial.NewClient("/dev/ttyDoesNotExist", 9600,
		serial.WithMode(modbus.SerialRTU),
		serial.WithTimeout(500*time.Millisecond),
	)
	if err != nil {
		// Some platforms reject the port at construction time — that
		// is also acceptable.
		return
	}
	defer c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Second)
	defer cancel()
	if err := c.Connect(ctx); err == nil {
		t.Fatal("expected Connect to fail against /dev/ttyDoesNotExist")
	}
}

// TestUseAfterCloseFails verifies request methods on a closed client
// return ErrClosed.
func TestUseAfterCloseFails(t *testing.T) {
	c, err := serial.NewClient("/dev/ttyS99", 9600)
	if err != nil {
		t.Skipf("cannot construct serial client: %v", err)
	}
	_ = c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = c.ReadHoldingRegisters(ctx, 1, 0, 1)
	if !errors.Is(err, modbus.ErrClosed) {
		t.Fatalf("want ErrClosed, got %v", err)
	}
}

// TestASCIIMode verifies WithMode(SerialASCII) is accepted.
func TestASCIIMode(t *testing.T) {
	c, err := serial.NewClient("/dev/ttyS99", 9600, serial.WithMode(modbus.SerialASCII))
	if err != nil {
		t.Skipf("cannot construct ASCII client: %v", err)
	}
	_ = c.Close()
}
