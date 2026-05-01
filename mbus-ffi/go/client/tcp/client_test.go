package tcp_test

import (
	"context"
	"errors"
	"testing"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// TestNewClientInvalidHost verifies that the constructor either rejects
// or accepts invalid host strings without panicking — the Rust side
// validates UTF-8 only, deferring resolution to Connect.
func TestNewClientInvalidHost(t *testing.T) {
	c, err := tcp.NewClient("\xff\xff", 502)
	if err == nil {
		_ = c.Close()
	}
	// Either outcome is acceptable as long as we did not panic.
}

// TestCloseIsIdempotent verifies that calling Close more than once is
// safe and returns nil.
func TestCloseIsIdempotent(t *testing.T) {
	c, err := tcp.NewClient("127.0.0.1", 1)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	if err := c.Close(); err != nil {
		t.Fatalf("first Close: %v", err)
	}
	if err := c.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}
}

// TestUseAfterCloseFails ensures that any request method on a closed
// client returns ErrClosed.
func TestUseAfterCloseFails(t *testing.T) {
	c, err := tcp.NewClient("127.0.0.1", 1)
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	_ = c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), time.Second)
	defer cancel()
	_, err = c.ReadHoldingRegisters(ctx, 1, 0, 1)
	if !errors.Is(err, modbus.ErrClosed) {
		t.Fatalf("want ErrClosed, got %v", err)
	}
}

// TestConnectToNothingFails verifies that connecting to a port nothing
// is listening on returns a sensible error within the request timeout.
func TestConnectToNothingFails(t *testing.T) {
	c, err := tcp.NewClient("127.0.0.1", 1, tcp.WithTimeout(500*time.Millisecond))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	if err := c.Connect(ctx); err == nil {
		t.Fatal("expected Connect to fail against an unreachable port")
	}
}
