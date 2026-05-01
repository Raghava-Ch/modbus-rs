// Package tcp provides an idiomatic Go async Modbus TCP server.
//
// Implement [Handler] (or embed [BaseHandler] and override only the
// methods you care about), then construct a [Server] and call
// [Server.Serve].
//
// # Example
//
//	type myHandler struct{ tcp.BaseHandler }
//
//	func (h *myHandler) ReadHoldingRegisters(ctx context.Context, _ uint8, addr, count uint16) ([]uint16, error) {
//	    return make([]uint16, count), nil
//	}
//
//	srv, err := tcp.NewServer("0.0.0.0:1502", &myHandler{})
//	if err != nil { log.Fatal(err) }
//	defer srv.Close()
//	if err := srv.Serve(ctx); err != nil { log.Fatal(err) }
package tcp

import (
	"context"
	"errors"
	"net"
	"runtime"
	rcgo "runtime/cgo"
	"strconv"
	"sync"
	"sync/atomic"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// Handler is the interface a user implements to back a Modbus TCP
// server. Each method corresponds to one Modbus function code; methods
// not overridden by embedding [BaseHandler] return an Illegal Function
// exception.
//
// Methods are invoked from native (Tokio) worker threads; they may
// freely call Go runtime services, but must avoid blocking arbitrarily.
type Handler interface {
	ReadCoils(ctx context.Context, unit uint8, addr, count uint16) ([]bool, error)
	WriteSingleCoil(ctx context.Context, unit uint8, addr uint16, value bool) error
	WriteMultipleCoils(ctx context.Context, unit uint8, addr uint16, values []bool) error
	ReadDiscreteInputs(ctx context.Context, unit uint8, addr, count uint16) ([]bool, error)
	ReadHoldingRegisters(ctx context.Context, unit uint8, addr, count uint16) ([]uint16, error)
	ReadInputRegisters(ctx context.Context, unit uint8, addr, count uint16) ([]uint16, error)
	WriteSingleRegister(ctx context.Context, unit uint8, addr, value uint16) error
	WriteMultipleRegisters(ctx context.Context, unit uint8, addr uint16, values []uint16) error
}

// BaseHandler is a default Handler that returns an Illegal Function
// exception for every operation. Embed it and override only the
// methods relevant to your device.
type BaseHandler struct{}

func (BaseHandler) ReadCoils(context.Context, uint8, uint16, uint16) ([]bool, error) {
	return nil, IllegalFunction()
}
func (BaseHandler) WriteSingleCoil(context.Context, uint8, uint16, bool) error {
	return IllegalFunction()
}
func (BaseHandler) WriteMultipleCoils(context.Context, uint8, uint16, []bool) error {
	return IllegalFunction()
}
func (BaseHandler) ReadDiscreteInputs(context.Context, uint8, uint16, uint16) ([]bool, error) {
	return nil, IllegalFunction()
}
func (BaseHandler) ReadHoldingRegisters(context.Context, uint8, uint16, uint16) ([]uint16, error) {
	return nil, IllegalFunction()
}
func (BaseHandler) ReadInputRegisters(context.Context, uint8, uint16, uint16) ([]uint16, error) {
	return nil, IllegalFunction()
}
func (BaseHandler) WriteSingleRegister(context.Context, uint8, uint16, uint16) error {
	return IllegalFunction()
}
func (BaseHandler) WriteMultipleRegisters(context.Context, uint8, uint16, []uint16) error {
	return IllegalFunction()
}

// ── Exception helpers ──────────────────────────────────────────────────────

// Exception is an error type that, when returned from a Handler method,
// produces a Modbus protocol-level exception with the given code.
type Exception struct {
	Code modbus.ExceptionCode
}

func (e *Exception) Error() string         { return "modbus exception: " + e.Code.String() }
func (e *Exception) ExceptionCode() uint8  { return uint8(e.Code) }

// IllegalFunction returns a sentinel error producing exception 0x01.
func IllegalFunction() error      { return &Exception{modbus.ExIllegalFunction} }

// IllegalDataAddress returns a sentinel error producing exception 0x02.
func IllegalDataAddress() error   { return &Exception{modbus.ExIllegalDataAddress} }

// IllegalDataValue returns a sentinel error producing exception 0x03.
func IllegalDataValue() error     { return &Exception{modbus.ExIllegalDataValue} }

// ServerDeviceFailure returns a sentinel error producing exception 0x04.
func ServerDeviceFailure() error  { return &Exception{modbus.ExServerDeviceFailure} }

// WithException wraps an arbitrary modbus.ExceptionCode.
func WithException(code modbus.ExceptionCode) error { return &Exception{code} }

// ── Server ─────────────────────────────────────────────────────────────────

// Server is the Modbus TCP server handle.
type Server struct {
	handle  atomic.Uintptr
	cbHand  rcgo.Handle
	closing sync.Mutex

	addr   string
	unitID uint8

	startedOnce sync.Once
	startErr    error
}

// Option configures a [Server] at construction time.
type Option func(*config)

type config struct {
	unitID uint8
}

// WithUnitID sets the Modbus unit ID this server responds to. Defaults
// to 1.
func WithUnitID(unit uint8) Option { return func(c *config) { c.unitID = unit } }

// NewServer constructs a TCP server bound to addr ("host:port"). The
// listener is NOT yet opened — call [Server.Serve] to begin accepting.
func NewServer(addr string, h Handler, opts ...Option) (*Server, error) {
	cfg := config{unitID: 1}
	for _, o := range opts {
		o(&cfg)
	}
	host, portStr, err := net.SplitHostPort(addr)
	if err != nil {
		return nil, &modbus.Error{Op: "NewServer", Status: modbus.StatusInvalidConfiguration, Cause: err}
	}
	port64, err := strconv.ParseUint(portStr, 10, 16)
	if err != nil {
		return nil, &modbus.Error{Op: "NewServer", Status: modbus.StatusInvalidConfiguration, Cause: err}
	}

	cb := newAdapter(h)
	srv, hand, st := cgo.TcpServerNew(host, uint16(port64), cfg.unitID, cb)
	if st != modbus.StatusOK {
		return nil, modbus.FromStatus("NewServer", st)
	}
	s := &Server{
		cbHand: hand,
		addr:   addr,
		unitID: cfg.unitID,
	}
	s.handle.Store(uintptr(toUintptr(srv)))
	runtime.SetFinalizer(s, func(s *Server) { _ = s.Close() })
	return s, nil
}

// Serve starts the listener and blocks until ctx is cancelled or
// [Server.Close] is called.
//
// Calling Serve more than once on the same Server is a no-op for the
// second and later calls (they just block on ctx).
func (s *Server) Serve(ctx context.Context) error {
	s.startedOnce.Do(func() {
		h := fromUintptr(s.handle.Load())
		if h == nil {
			s.startErr = modbus.ErrClosed
			return
		}
		st := cgo.TcpServerStart(h)
		if st != modbus.StatusOK {
			s.startErr = modbus.FromStatus("Serve", st)
		}
	})
	if s.startErr != nil {
		return s.startErr
	}
	if ctx == nil {
		// Block forever — the only way to exit is Close().
		select {}
	}
	<-ctx.Done()
	return ctx.Err()
}

// Close stops the listener and frees all native resources. Safe to
// call concurrently or multiple times.
func (s *Server) Close() error {
	s.closing.Lock()
	defer s.closing.Unlock()
	old := s.handle.Swap(0)
	if old == 0 {
		return nil
	}
	h := fromUintptr(old)
	cgo.TcpServerStop(h)
	cgo.TcpServerFree(h, s.cbHand)
	s.cbHand = 0
	runtime.SetFinalizer(s, nil)
	return nil
}

// Addr returns the originally-configured listen address.
func (s *Server) Addr() string { return s.addr }

// ── Internals ──────────────────────────────────────────────────────────────

// adapter bridges the cgo.ServerCallbacks vtable interface (no
// context.Context, no unit ID) to the public Handler interface (with
// both). The native server stores the unit ID it was constructed with
// and only invokes us when an incoming request matches it.
type adapter struct {
	h      Handler
	unitID uint8
}

func newAdapter(h Handler) *adapter {
	return &adapter{h: h, unitID: 1}
}

// All adapter methods are invoked on a Tokio worker thread. We use a
// background context for the inner Handler call; deadline propagation
// is on the roadmap.
func (a *adapter) ReadCoils(addr, count uint16) ([]bool, error) {
	return a.h.ReadCoils(context.Background(), a.unitID, addr, count)
}
func (a *adapter) WriteSingleCoil(addr uint16, value bool) error {
	return a.h.WriteSingleCoil(context.Background(), a.unitID, addr, value)
}
func (a *adapter) WriteMultipleCoils(addr uint16, values []bool) error {
	return a.h.WriteMultipleCoils(context.Background(), a.unitID, addr, values)
}
func (a *adapter) ReadDiscreteInputs(addr, count uint16) ([]bool, error) {
	return a.h.ReadDiscreteInputs(context.Background(), a.unitID, addr, count)
}
func (a *adapter) ReadHoldingRegisters(addr, count uint16) ([]uint16, error) {
	return a.h.ReadHoldingRegisters(context.Background(), a.unitID, addr, count)
}
func (a *adapter) ReadInputRegisters(addr, count uint16) ([]uint16, error) {
	return a.h.ReadInputRegisters(context.Background(), a.unitID, addr, count)
}
func (a *adapter) WriteSingleRegister(addr, value uint16) error {
	return a.h.WriteSingleRegister(context.Background(), a.unitID, addr, value)
}
func (a *adapter) WriteMultipleRegisters(addr uint16, values []uint16) error {
	return a.h.WriteMultipleRegisters(context.Background(), a.unitID, addr, values)
}

// Compile-time assertion that *adapter satisfies cgo.ServerCallbacks.
var _ cgo.ServerCallbacks = (*adapter)(nil)

// Compile-time assertion that BaseHandler satisfies Handler.
var _ Handler = BaseHandler{}

// Sentinel that errors.Is can match.
var errClosed = errors.New("server: closed")
