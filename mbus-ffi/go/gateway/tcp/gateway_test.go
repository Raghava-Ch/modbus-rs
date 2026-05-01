package tcp_test

import (
	"context"
	"fmt"
	"net"
	"testing"
	"time"

	gwtcp "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/gateway/tcp"
)

func pickPort(t *testing.T) uint16 {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("pickPort: %v", err)
	}
	defer l.Close()
	return uint16(l.Addr().(*net.TCPAddr).Port)
}

// TestRouterBuilder verifies the fluent Router builder accepts unit
// and range routes without error. The actual proxying behaviour is
// covered by the workspace-level gateway integration tests.
func TestRouterBuilder(t *testing.T) {
	r := gwtcp.NewRouter().
		AddUnit(1, 0).
		AddUnit(2, 1).
		AddRange(10, 20, 0)
	if r == nil {
		t.Fatal("NewRouter returned nil")
	}
}

// TestNewGatewayValidatesChannelIndex ensures an out-of-range channel
// index in the router yields a clean error rather than a panic.
func TestNewGatewayValidatesChannelIndex(t *testing.T) {
	port := pickPort(t)
	r := gwtcp.NewRouter().AddUnit(1, 99) // channel 99 doesn't exist
	_, err := gwtcp.NewGateway(
		fmt.Sprintf("127.0.0.1:%d", port),
		[]gwtcp.Downstream{{Host: "127.0.0.1", Port: 502}},
		r,
	)
	if err == nil {
		t.Fatal("expected NewGateway to reject out-of-range channel")
	}
}

// TestGatewayStartStop spins up an empty-route gateway, starts the
// listener, and tears it down cleanly. Even with no traffic, this
// exercises the native start/stop lifecycle.
func TestGatewayStartStop(t *testing.T) {
	port := pickPort(t)
	r := gwtcp.NewRouter().AddUnit(1, 0)
	gw, err := gwtcp.NewGateway(
		fmt.Sprintf("127.0.0.1:%d", port),
		[]gwtcp.Downstream{{Host: "127.0.0.1", Port: 65000}},
		r,
	)
	if err != nil {
		t.Fatalf("NewGateway: %v", err)
	}
	defer gw.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 250*time.Millisecond)
	defer cancel()
	_ = gw.Serve(ctx) // returns ctx.Err() (DeadlineExceeded) when ctx fires
	if err := gw.Close(); err != nil {
		t.Fatalf("Close: %v", err)
	}
}
