package tcp_test

import (
	"context"
	"fmt"
	"net"
	"sync"
	"testing"
	"time"

	clienttcp "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/server/tcp"
)

// in-memory device that backs every test below.
type memDevice struct {
	tcp.BaseHandler
	mu       sync.Mutex
	holding  [256]uint16
	coils    [256]bool
}

func (d *memDevice) ReadHoldingRegisters(_ context.Context, _ uint8, addr, count uint16) ([]uint16, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr)+int(count) > len(d.holding) {
		return nil, tcp.IllegalDataAddress()
	}
	out := make([]uint16, count)
	copy(out, d.holding[addr:addr+count])
	return out, nil
}

func (d *memDevice) WriteSingleRegister(_ context.Context, _ uint8, addr, value uint16) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr) >= len(d.holding) {
		return tcp.IllegalDataAddress()
	}
	d.holding[addr] = value
	return nil
}

func (d *memDevice) WriteMultipleRegisters(_ context.Context, _ uint8, addr uint16, values []uint16) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr)+len(values) > len(d.holding) {
		return tcp.IllegalDataAddress()
	}
	copy(d.holding[addr:], values)
	return nil
}

func (d *memDevice) ReadCoils(_ context.Context, _ uint8, addr, count uint16) ([]bool, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr)+int(count) > len(d.coils) {
		return nil, tcp.IllegalDataAddress()
	}
	out := make([]bool, count)
	copy(out, d.coils[addr:addr+count])
	return out, nil
}

func (d *memDevice) WriteSingleCoil(_ context.Context, _ uint8, addr uint16, value bool) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr) >= len(d.coils) {
		return tcp.IllegalDataAddress()
	}
	d.coils[addr] = value
	return nil
}

// pickPort returns a free localhost TCP port.
func pickPort(t *testing.T) uint16 {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("pickPort: %v", err)
	}
	defer l.Close()
	return uint16(l.Addr().(*net.TCPAddr).Port)
}

func TestServerNewAndClose(t *testing.T) {
	port := pickPort(t)
	srv, err := tcp.NewServer(fmt.Sprintf("127.0.0.1:%d", port), &memDevice{})
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}
	if err := srv.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
	if err := srv.Close(); err != nil {
		t.Fatalf("second Close: %v", err)
	}
}

func TestServerRoundTrip(t *testing.T) {
	port := pickPort(t)
	dev := &memDevice{}
	srv, err := tcp.NewServer(fmt.Sprintf("127.0.0.1:%d", port), dev)
	if err != nil {
		t.Fatalf("NewServer: %v", err)
	}
	defer srv.Close()

	// Run Serve in the background.
	srvCtx, srvCancel := context.WithCancel(context.Background())
	defer srvCancel()
	srvErr := make(chan error, 1)
	go func() { srvErr <- srv.Serve(srvCtx) }()

	// Give the listener a moment to come up.
	time.Sleep(150 * time.Millisecond)

	// ── Client ─────────────────────────────────────────────────────────
	c, err := clienttcp.NewClient("127.0.0.1", port, clienttcp.WithTimeout(2*time.Second))
	if err != nil {
		t.Fatalf("NewClient: %v", err)
	}
	defer c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := c.Connect(ctx); err != nil {
		t.Fatalf("Connect: %v", err)
	}

	// FC06 → FC03
	if err := c.WriteSingleRegister(ctx, 1, 5, 0xBEEF); err != nil {
		t.Fatalf("WriteSingleRegister: %v", err)
	}
	got, err := c.ReadHoldingRegisters(ctx, 1, 5, 1)
	if err != nil {
		t.Fatalf("ReadHoldingRegisters: %v", err)
	}
	if len(got) != 1 || got[0] != 0xBEEF {
		t.Fatalf("got %v, want [0xBEEF]", got)
	}

	// FC16 → FC03
	want := []uint16{0xAA, 0xBB, 0xCC, 0xDD}
	if err := c.WriteMultipleRegisters(ctx, 1, 20, want); err != nil {
		t.Fatalf("WriteMultipleRegisters: %v", err)
	}
	got2, err := c.ReadHoldingRegisters(ctx, 1, 20, uint16(len(want)))
	if err != nil {
		t.Fatalf("ReadHoldingRegisters (multi): %v", err)
	}
	for i := range want {
		if got2[i] != want[i] {
			t.Fatalf("idx %d: got %d, want %d", i, got2[i], want[i])
		}
	}

	// FC05 → FC01
	if err := c.WriteSingleCoil(ctx, 1, 3, true); err != nil {
		t.Fatalf("WriteSingleCoil: %v", err)
	}
	bits, err := c.ReadCoils(ctx, 1, 0, 5)
	if err != nil {
		t.Fatalf("ReadCoils: %v", err)
	}
	if len(bits) != 5 {
		t.Fatalf("want 5 bits, got %d", len(bits))
	}
	if !bits[3] {
		t.Fatalf("expected coil 3 to be set; got %v", bits)
	}
	for i, b := range bits {
		if i == 3 {
			continue
		}
		if b {
			t.Fatalf("coil %d set unexpectedly: %v", i, bits)
		}
	}

	// Exception path: read out-of-range address should yield IllegalDataAddress.
	_, err = c.ReadHoldingRegisters(ctx, 1, 1000, 1)
	if err == nil {
		t.Fatal("expected error for out-of-range read")
	}
	var mErr *modbus.Error
	if !errorAs(err, &mErr) || mErr.Status != modbus.StatusModbusException {
		t.Fatalf("want modbus.Error with StatusModbusException, got %T %v", err, err)
	}
}

// errorAs is a small wrapper around errors.As to keep the test
// readable.
func errorAs(err error, target any) bool {
	type asErr interface{ As(any) bool }
	_ = asErr(nil)
	if e, ok := err.(*modbus.Error); ok {
		if t, ok := target.(**modbus.Error); ok {
			*t = e
			return true
		}
	}
	return false
}
