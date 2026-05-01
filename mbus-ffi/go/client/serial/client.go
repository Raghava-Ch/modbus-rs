// Package serial provides an idiomatic Go async Modbus serial (RTU/ASCII) client.
package serial

import (
	"context"
	"runtime"
	"sync"
	"sync/atomic"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// Client is an idiomatic Go async Modbus serial client.
type Client struct {
	handle  atomic.Pointer[cgo.SerialClient]
	closing sync.RWMutex
	timeout time.Duration
}

// Option configures a [Client] at construction time.
type Option func(*config)

type config struct {
	mode     modbus.SerialMode
	dataBits uint8
	parity   uint8 // 0=None, 1=Odd, 2=Even
	stopBits uint8 // 1 or 2
	respMs   uint32
	timeout  time.Duration
}

// WithMode selects RTU vs ASCII framing. Default RTU.
func WithMode(m modbus.SerialMode) Option { return func(c *config) { c.mode = m } }

// WithDataBits sets data bits. Default 8.
func WithDataBits(n uint8) Option { return func(c *config) { c.dataBits = n } }

// WithParity sets parity (0=None, 1=Odd, 2=Even). Default None.
func WithParity(p uint8) Option { return func(c *config) { c.parity = p } }

// WithStopBits sets stop bits. Default 1.
func WithStopBits(s uint8) Option { return func(c *config) { c.stopBits = s } }

// WithResponseTimeoutMs sets the per-byte response timeout. Default 1000.
func WithResponseTimeoutMs(ms uint32) Option { return func(c *config) { c.respMs = ms } }

// WithTimeout sets the per-request high-level timeout. Default 5s.
func WithTimeout(d time.Duration) Option { return func(c *config) { c.timeout = d } }

// NewClient constructs a serial client. The native transport is NOT
// yet opened — call [Client.Connect] for that.
func NewClient(port string, baud uint32, opts ...Option) (*Client, error) {
	cfg := config{
		mode:     modbus.SerialRTU,
		dataBits: 8,
		parity:   0,
		stopBits: 1,
		respMs:   1000,
		timeout:  5 * time.Second,
	}
	for _, o := range opts {
		o(&cfg)
	}

	var h *cgo.SerialClient
	switch cfg.mode {
	case modbus.SerialASCII:
		h = cgo.SerialClientNewASCII(port, baud, cfg.dataBits, cfg.parity, cfg.stopBits, cfg.respMs)
	default:
		h = cgo.SerialClientNewRTU(port, baud, cfg.dataBits, cfg.parity, cfg.stopBits, cfg.respMs)
	}
	if h == nil {
		return nil, modbus.FromStatus("NewClient", modbus.StatusInvalidConfiguration)
	}
	c := &Client{timeout: cfg.timeout}
	c.handle.Store(h)
	if cfg.timeout > 0 {
		cgo.SerialClientSetRequestTimeoutMs(h, uint64(cfg.timeout/time.Millisecond))
	}
	runtime.SetFinalizer(c, func(c *Client) { _ = c.Close() })
	return c, nil
}

// Close releases the native handle.
func (c *Client) Close() error {
	c.closing.Lock()
	defer c.closing.Unlock()
	old := c.handle.Swap(nil)
	if old == nil {
		return nil
	}
	cgo.SerialClientFree(old)
	runtime.SetFinalizer(c, nil)
	return nil
}

// Connect opens the underlying serial port.
func (c *Client) Connect(ctx context.Context) error {
	return c.do(ctx, "Connect", func(h *cgo.SerialClient) modbus.Status {
		return cgo.SerialClientConnect(h)
	})
}

// Disconnect closes the serial port without freeing the handle.
func (c *Client) Disconnect(ctx context.Context) error {
	return c.do(ctx, "Disconnect", func(h *cgo.SerialClient) modbus.Status {
		return cgo.SerialClientDisconnect(h)
	})
}

// ReadHoldingRegisters performs FC03.
func (c *Client) ReadHoldingRegisters(ctx context.Context, unit uint8, addr, qty uint16) ([]uint16, error) {
	var out []uint16
	err := c.do(ctx, "ReadHoldingRegisters", func(h *cgo.SerialClient) modbus.Status {
		var st modbus.Status
		out, st = cgo.SerialClientReadHoldingRegisters(h, unit, addr, qty)
		return st
	})
	return out, err
}

// WriteSingleRegister performs FC06.
func (c *Client) WriteSingleRegister(ctx context.Context, unit uint8, addr, value uint16) error {
	return c.do(ctx, "WriteSingleRegister", func(h *cgo.SerialClient) modbus.Status {
		return cgo.SerialClientWriteSingleRegister(h, unit, addr, value)
	})
}

func (c *Client) do(ctx context.Context, op string, f func(*cgo.SerialClient) modbus.Status) error {
	c.closing.RLock()
	defer c.closing.RUnlock()
	h := c.handle.Load()
	if h == nil {
		return &modbus.Error{Op: op, Status: modbus.StatusNullPointer, Cause: modbus.ErrClosed}
	}

	if ctx != nil {
		if err := ctx.Err(); err != nil {
			return err
		}
	}

	done := make(chan modbus.Status, 1)
	go func() { done <- f(h) }()

	if ctx == nil {
		return modbus.FromStatus(op, <-done)
	}
	select {
	case st := <-done:
		return modbus.FromStatus(op, st)
	case <-ctx.Done():
		return ctx.Err()
	}
}
