// Demonstrates the Go Modbus TCP server with a small in-memory device.
//
// Usage:
//
//	go run ./examples/tcp-server -listen 127.0.0.1:1502
package main

import (
	"context"
	"flag"
	"log"
	"os"
	"os/signal"
	"sync"
	"syscall"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/server/tcp"
)

// device is a tiny in-memory backing store backing all FCs we override.
type device struct {
	tcp.BaseHandler
	mu      sync.Mutex
	holding [256]uint16
}

func (d *device) ReadHoldingRegisters(_ context.Context, _ uint8, addr, count uint16) ([]uint16, error) {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr)+int(count) > len(d.holding) {
		return nil, tcp.IllegalDataAddress()
	}
	out := make([]uint16, count)
	copy(out, d.holding[addr:addr+count])
	return out, nil
}

func (d *device) WriteSingleRegister(_ context.Context, _ uint8, addr, value uint16) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr) >= len(d.holding) {
		return tcp.IllegalDataAddress()
	}
	d.holding[addr] = value
	return nil
}

func (d *device) WriteMultipleRegisters(_ context.Context, _ uint8, addr uint16, values []uint16) error {
	d.mu.Lock()
	defer d.mu.Unlock()
	if int(addr)+len(values) > len(d.holding) {
		return tcp.IllegalDataAddress()
	}
	copy(d.holding[addr:], values)
	return nil
}

func main() {
	addr := flag.String("listen", "127.0.0.1:1502", "listen address")
	flag.Parse()

	srv, err := tcp.NewServer(*addr, &device{})
	if err != nil {
		log.Fatalf("NewServer: %v", err)
	}
	defer srv.Close()

	// Cancel on SIGINT/SIGTERM.
	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	log.Printf("Modbus TCP server listening on %s", srv.Addr())
	if err := srv.Serve(ctx); err != nil && err != context.Canceled {
		log.Fatalf("Serve: %v", err)
	}
	log.Println("shutting down")
}
