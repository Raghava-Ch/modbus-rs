// Package tcp provides an idiomatic Go async Modbus TCP gateway.
//
// A gateway accepts incoming Modbus TCP connections and forwards each
// request to one of several downstream Modbus servers (e.g. RTU
// devices behind serial-to-TCP converters), based on the unit-id
// routing rules registered through [Router].
//
// # Example
//
//	r := tcp.NewRouter().
//	    AddUnit(1, 0).      // unit 1 → channel 0
//	    AddRange(2, 10, 1)  // units 2..10 → channel 1
//
//	gw, err := tcp.NewGateway("0.0.0.0:1502",
//	    []tcp.Downstream{
//	        {Host: "192.168.1.10", Port: 502},
//	        {Host: "192.168.1.11", Port: 502},
//	    }, r)
//	if err != nil { log.Fatal(err) }
//	defer gw.Close()
//	if err := gw.Serve(ctx); err != nil { log.Fatal(err) }
package tcp

import (
	"context"
	"net"
	"runtime"
	"strconv"
	"sync"
	"sync/atomic"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/internal/cgo"
	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/modbus"
)

// Downstream identifies a downstream Modbus TCP server.
type Downstream struct {
	Host string
	Port uint16
}

// Router is a builder for unit-id → downstream-channel routing rules.
//
// Channel indices refer to the order in which downstreams are passed
// to [NewGateway].
type Router struct {
	units  []unitRoute
	ranges []rangeRoute
}

type unitRoute struct {
	unit    uint8
	channel int
}

type rangeRoute struct {
	min, max uint8
	channel  int
}

// NewRouter constructs an empty router.
func NewRouter() *Router { return &Router{} }

// AddUnit routes a single unit-id to the downstream at the given
// channel index.
func (r *Router) AddUnit(unit uint8, channel int) *Router {
	r.units = append(r.units, unitRoute{unit, channel})
	return r
}

// AddRange routes the inclusive range [min, max] to the downstream at
// the given channel index.
func (r *Router) AddRange(min, max uint8, channel int) *Router {
	r.ranges = append(r.ranges, rangeRoute{min, max, channel})
	return r
}

// Server is a Modbus TCP gateway handle.
type Server struct {
	handle      atomic.Pointer[cgo.TcpGateway]
	addr        string
	closing     sync.Mutex
	startedOnce sync.Once
	startErr    error
}

// NewGateway constructs a TCP gateway listening on listenAddr that
// proxies requests to the given downstreams using router `r`.
func NewGateway(listenAddr string, downstreams []Downstream, r *Router) (*Server, error) {
	host, portStr, err := net.SplitHostPort(listenAddr)
	if err != nil {
		return nil, &modbus.Error{Op: "NewGateway", Status: modbus.StatusInvalidConfiguration, Cause: err}
	}
	port64, err := strconv.ParseUint(portStr, 10, 16)
	if err != nil {
		return nil, &modbus.Error{Op: "NewGateway", Status: modbus.StatusInvalidConfiguration, Cause: err}
	}

	g := cgo.TcpGatewayNew(host, uint16(port64))
	if g == nil {
		return nil, modbus.FromStatus("NewGateway", modbus.StatusInvalidConfiguration)
	}

	// Register downstreams; capture channel indices.
	channels := make([]uint32, len(downstreams))
	for i, d := range downstreams {
		channels[i] = cgo.TcpGatewayAddDownstream(g, d.Host, d.Port)
	}

	if r == nil {
		r = NewRouter()
	}
	for _, u := range r.units {
		if u.channel < 0 || u.channel >= len(channels) {
			cgo.TcpGatewayFree(g)
			return nil, &modbus.Error{Op: "NewGateway", Status: modbus.StatusInvalidConfiguration}
		}
		st := cgo.TcpGatewayAddUnitRoute(g, u.unit, channels[u.channel])
		if st != modbus.StatusOK {
			cgo.TcpGatewayFree(g)
			return nil, modbus.FromStatus("NewGateway/AddUnit", st)
		}
	}
	for _, rg := range r.ranges {
		if rg.channel < 0 || rg.channel >= len(channels) {
			cgo.TcpGatewayFree(g)
			return nil, &modbus.Error{Op: "NewGateway", Status: modbus.StatusInvalidConfiguration}
		}
		st := cgo.TcpGatewayAddRangeRoute(g, rg.min, rg.max, channels[rg.channel])
		if st != modbus.StatusOK {
			cgo.TcpGatewayFree(g)
			return nil, modbus.FromStatus("NewGateway/AddRange", st)
		}
	}

	s := &Server{addr: listenAddr}
	s.handle.Store(g)
	runtime.SetFinalizer(s, func(s *Server) { _ = s.Close() })
	return s, nil
}

// Serve starts the listener thread and blocks until ctx is cancelled
// or [Server.Close] is called.
func (s *Server) Serve(ctx context.Context) error {
	s.startedOnce.Do(func() {
		h := s.handle.Load()
		if h == nil {
			s.startErr = modbus.ErrClosed
			return
		}
		st := cgo.TcpGatewayStart(h)
		if st != modbus.StatusOK {
			s.startErr = modbus.FromStatus("Serve", st)
		}
	})
	if s.startErr != nil {
		return s.startErr
	}
	if ctx == nil {
		select {}
	}
	<-ctx.Done()
	return ctx.Err()
}

// Close stops the gateway and releases native resources.
func (s *Server) Close() error {
	s.closing.Lock()
	defer s.closing.Unlock()
	old := s.handle.Swap(nil)
	if old == nil {
		return nil
	}
	cgo.TcpGatewayStop(old)
	cgo.TcpGatewayFree(old)
	runtime.SetFinalizer(s, nil)
	return nil
}

// Addr returns the originally-configured listen address.
func (s *Server) Addr() string { return s.addr }
