// Demonstrates the Go Modbus TCP client.
//
// Usage:
//
//	go run ./examples/tcp-client -host 127.0.0.1 -port 1502 -unit 1
package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
)

func main() {
	host := flag.String("host", "127.0.0.1", "Modbus TCP server host")
	port := flag.Uint("port", 1502, "Modbus TCP server port")
	unit := flag.Uint("unit", 1, "Modbus unit ID")
	flag.Parse()

	c, err := tcp.NewClient(*host, uint16(*port), tcp.WithTimeout(2*time.Second))
	if err != nil {
		log.Fatalf("NewClient: %v", err)
	}
	defer c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := c.Connect(ctx); err != nil {
		log.Fatalf("Connect: %v", err)
	}

	if err := c.WriteSingleRegister(ctx, uint8(*unit), 0, 0xCAFE); err != nil {
		log.Fatalf("WriteSingleRegister: %v", err)
	}

	regs, err := c.ReadHoldingRegisters(ctx, uint8(*unit), 0, 4)
	if err != nil {
		log.Fatalf("ReadHoldingRegisters: %v", err)
	}
	fmt.Printf("Holding registers [0..3] = %v\n", regs)
}
