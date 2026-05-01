// Demonstrates the Go Modbus TCP gateway with two downstream servers
// and a small routing table.
//
// Usage:
//
//	go run ./examples/tcp-gateway -listen 127.0.0.1:5020 \
//	    -down 192.168.1.10:502,192.168.1.11:502
package main

import (
	"context"
	"flag"
	"log"
	"net"
	"os"
	"os/signal"
	"strconv"
	"strings"
	"syscall"

	gwtcp "github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/gateway/tcp"
)

func parseDownstreams(s string) ([]gwtcp.Downstream, error) {
	parts := strings.Split(s, ",")
	out := make([]gwtcp.Downstream, 0, len(parts))
	for _, p := range parts {
		host, portStr, err := net.SplitHostPort(strings.TrimSpace(p))
		if err != nil {
			return nil, err
		}
		port, err := strconv.ParseUint(portStr, 10, 16)
		if err != nil {
			return nil, err
		}
		out = append(out, gwtcp.Downstream{Host: host, Port: uint16(port)})
	}
	return out, nil
}

func main() {
	listen := flag.String("listen", "127.0.0.1:5020", "listen address")
	downs := flag.String("down", "127.0.0.1:1502", "comma-separated downstream host:port list")
	flag.Parse()

	downstreams, err := parseDownstreams(*downs)
	if err != nil {
		log.Fatalf("invalid -down: %v", err)
	}

	// Default routing: unit 1 → channel 0, units 2..10 → channel 1
	// (only used if at least two downstreams exist).
	r := gwtcp.NewRouter().AddUnit(1, 0)
	if len(downstreams) > 1 {
		r.AddRange(2, 10, 1)
	}

	gw, err := gwtcp.NewGateway(*listen, downstreams, r)
	if err != nil {
		log.Fatalf("NewGateway: %v", err)
	}
	defer gw.Close()

	ctx, cancel := signal.NotifyContext(context.Background(), os.Interrupt, syscall.SIGTERM)
	defer cancel()

	log.Printf("Modbus TCP gateway listening on %s", gw.Addr())
	if err := gw.Serve(ctx); err != nil && err != context.Canceled {
		log.Fatalf("Serve: %v", err)
	}
	log.Println("shutting down")
}
