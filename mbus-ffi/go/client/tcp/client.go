// Package tcp provides an idiomatic Go async Modbus TCP client over the
// modbus-rs `mbus_ffi` cdylib.
//
// # Example
//
//	c, err := tcp.NewClient("127.0.0.1", 1502)
//	if err != nil { log.Fatal(err) }
//	defer c.Close()
//	if err := c.Connect(ctx); err != nil { log.Fatal(err) }
//
//	values, err := c.ReadHoldingRegisters(ctx, 1, 0, 10)
//	if err != nil { log.Fatal(err) }
//	fmt.Println(values)
package tcp

import (
	"context"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// Client is an idiomatic Go async Modbus TCP client.
//
// All methods are safe to call concurrently from multiple goroutines.
// Use [Client.Close] (or rely on the runtime finalizer) to release
// native resources.
type Client struct {
	// handle owns the native pointer. Stored as a uintptr behind atomic
	// access so [Client.Close] can be called concurrently with in-flight
	// requests.
	handle  atomic.Uintptr
	closing sync.RWMutex
	unitID  uint8
	timeout time.Duration
}

// Option configures a [Client] at construction time.
type Option func(*config)

type config struct {
	unitID  uint8
	timeout time.Duration
}

// WithUnitID sets the default Modbus unit ID used when an explicit
// argument is not provided. Defaults to 1.
func WithUnitID(unit uint8) Option { return func(c *config) { c.unitID = unit } }

// WithTimeout sets the per-request timeout. Defaults to 5s. Pass 0 to
// disable timeouts.
func WithTimeout(d time.Duration) Option { return func(c *config) { c.timeout = d } }

// NewClient creates a new TCP client targeting host:port. The native
// transport is NOT yet opened — call [Client.Connect] for that.
func NewClient(host string, port uint16, opts ...Option) (*Client, error) {
	cfg := config{unitID: 1, timeout: 5 * time.Second}
	for _, o := range opts {
		o(&cfg)
	}
	h := cgo.TcpClientNew(host, port)
	if h == nil {
		return nil, modbus.FromStatus("NewClient", modbus.StatusInvalidConfiguration)
	}
	c := &Client{unitID: cfg.unitID, timeout: cfg.timeout}
	c.handle.Store(uintptr(toUintptr(h)))
	if cfg.timeout > 0 {
		_ = cgo.TcpClientSetRequestTimeoutMs(h, uint64(cfg.timeout/time.Millisecond))
	}
	runtime.SetFinalizer(c, func(c *Client) { _ = c.Close() })
	return c, nil
}

// Close releases the native handle. Safe to call multiple times. Safe
// to call concurrently with in-flight requests; they will return
// [modbus.ErrClosed].
func (c *Client) Close() error {
	c.closing.Lock()
	defer c.closing.Unlock()
	old := c.handle.Swap(0)
	if old == 0 {
		return nil
	}
	cgo.TcpClientFree(fromUintptr(old))
	runtime.SetFinalizer(c, nil)
	return nil
}

// Connect opens the underlying TCP transport.
func (c *Client) Connect(ctx context.Context) error {
	return c.do(ctx, "Connect", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientConnect(h)
	})
}

// Disconnect closes the underlying TCP transport without freeing the
// handle. The client may be re-connected later.
func (c *Client) Disconnect(ctx context.Context) error {
	return c.do(ctx, "Disconnect", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientDisconnect(h)
	})
}

// HasPendingRequests reports whether requests are currently in flight.
func (c *Client) HasPendingRequests() bool {
	c.closing.RLock()
	defer c.closing.RUnlock()
	h := fromUintptr(c.handle.Load())
	if h == nil {
		return false
	}
	return cgo.TcpClientHasPendingRequests(h)
}

// SetRequestTimeout updates the per-request timeout. Pass 0 to disable.
func (c *Client) SetRequestTimeout(d time.Duration) error {
	return c.do(context.Background(), "SetRequestTimeout", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientSetRequestTimeoutMs(h, uint64(d/time.Millisecond))
	})
}

// ── Function-code methods ──────────────────────────────────────────────────

// ReadHoldingRegisters performs FC03.
func (c *Client) ReadHoldingRegisters(ctx context.Context, unit uint8, addr, qty uint16) ([]uint16, error) {
	var out []uint16
	err := c.do(ctx, "ReadHoldingRegisters", func(h *cgo.TcpClient) modbus.Status {
		var st modbus.Status
		out, st = cgo.TcpClientReadHoldingRegisters(h, unit, addr, qty)
		return st
	})
	return out, err
}

// ReadInputRegisters performs FC04.
func (c *Client) ReadInputRegisters(ctx context.Context, unit uint8, addr, qty uint16) ([]uint16, error) {
	var out []uint16
	err := c.do(ctx, "ReadInputRegisters", func(h *cgo.TcpClient) modbus.Status {
		var st modbus.Status
		out, st = cgo.TcpClientReadInputRegisters(h, unit, addr, qty)
		return st
	})
	return out, err
}

// WriteSingleRegister performs FC06.
func (c *Client) WriteSingleRegister(ctx context.Context, unit uint8, addr, value uint16) error {
	return c.do(ctx, "WriteSingleRegister", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientWriteSingleRegister(h, unit, addr, value)
	})
}

// WriteMultipleRegisters performs FC10.
func (c *Client) WriteMultipleRegisters(ctx context.Context, unit uint8, addr uint16, values []uint16) error {
	return c.do(ctx, "WriteMultipleRegisters", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientWriteMultipleRegisters(h, unit, addr, values)
	})
}

// MaskWriteRegister performs FC22.
func (c *Client) MaskWriteRegister(ctx context.Context, unit uint8, addr, andMask, orMask uint16) error {
	return c.do(ctx, "MaskWriteRegister", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientMaskWriteRegister(h, unit, addr, andMask, orMask)
	})
}

// ReadCoils performs FC01 and returns the bits unpacked as a []bool of
// length `qty`.
func (c *Client) ReadCoils(ctx context.Context, unit uint8, addr, qty uint16) ([]bool, error) {
	var packed []byte
	err := c.do(ctx, "ReadCoils", func(h *cgo.TcpClient) modbus.Status {
		var st modbus.Status
		packed, st = cgo.TcpClientReadCoils(h, unit, addr, qty)
		return st
	})
	if err != nil {
		return nil, err
	}
	return unpackBits(packed, int(qty)), nil
}

// ReadDiscreteInputs performs FC02 and returns the bits unpacked as a
// []bool of length `qty`.
func (c *Client) ReadDiscreteInputs(ctx context.Context, unit uint8, addr, qty uint16) ([]bool, error) {
	var packed []byte
	err := c.do(ctx, "ReadDiscreteInputs", func(h *cgo.TcpClient) modbus.Status {
		var st modbus.Status
		packed, st = cgo.TcpClientReadDiscreteInputs(h, unit, addr, qty)
		return st
	})
	if err != nil {
		return nil, err
	}
	return unpackBits(packed, int(qty)), nil
}

// WriteSingleCoil performs FC05.
func (c *Client) WriteSingleCoil(ctx context.Context, unit uint8, addr uint16, value bool) error {
	return c.do(ctx, "WriteSingleCoil", func(h *cgo.TcpClient) modbus.Status {
		return cgo.TcpClientWriteSingleCoil(h, unit, addr, value)
	})
}

// WriteMultipleCoils performs FC0F. The Modbus wire format packs `len(values)`
// coil bits into ceil(len/8) bytes, LSB first.
func (c *Client) WriteMultipleCoils(ctx context.Context, unit uint8, addr uint16, values []bool) error {
	return c.do(ctx, "WriteMultipleCoils", func(h *cgo.TcpClient) modbus.Status {
		packed := packBits(values)
		return cgo.TcpClientWriteMultipleCoils(h, unit, addr, packed, uint16(len(values)))
	})
}

// ── Internals ──────────────────────────────────────────────────────────────

// do takes the closing read-lock, fetches the native handle, and runs
// `f` on it. If the context is cancelled before `f` returns, the
// context error is returned (the in-flight native request is NOT
// interrupted; cancellation propagation is on the roadmap).
func (c *Client) do(ctx context.Context, op string, f func(*cgo.TcpClient) modbus.Status) error {
	c.closing.RLock()
	defer c.closing.RUnlock()
	h := fromUintptr(c.handle.Load())
	if h == nil {
		return &modbus.Error{Op: op, Status: modbus.StatusNullPointer, Cause: modbus.ErrClosed}
	}

	// Fast-path: pre-call context check.
	if ctx != nil {
		if err := ctx.Err(); err != nil {
			return err
		}
	}

	type result struct{ st modbus.Status }
	done := make(chan result, 1)
	go func() { done <- result{f(h)} }()

	if ctx == nil {
		return modbus.FromStatus(op, (<-done).st)
	}
	select {
	case r := <-done:
		return modbus.FromStatus(op, r.st)
	case <-ctx.Done():
		// The native request continues running in the background; its
		// result is dropped on the floor. Step 7 of the rollout plan
		// adds a cancel-token arg to the FFI to wire this through.
		return ctx.Err()
	}
}

func unpackBits(packed []byte, n int) []bool {
	out := make([]bool, n)
	for i := 0; i < n; i++ {
		if i/8 >= len(packed) {
			break
		}
		out[i] = (packed[i/8] & (1 << uint(i%8))) != 0
	}
	return out
}

func packBits(bits []bool) []byte {
	if len(bits) == 0 {
		return nil
	}
	out := make([]byte, (len(bits)+7)/8)
	for i, b := range bits {
		if b {
			out[i/8] |= 1 << uint(i%8)
		}
	}
	return out
}
