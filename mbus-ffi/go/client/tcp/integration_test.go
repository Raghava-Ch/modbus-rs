//go:build integration

package tcp_test

import (
	"bufio"
	"context"
	"os"
	"os/exec"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/Raghava-Ch/modbus-rs/mbus-ffi/go/client/tcp"
)

// startTestServer launches the existing Rust `dotnet_test_server`
// example as a subprocess and returns its TCP port plus a stop func.
//
// The example prints `LISTENING <port>\n` to stdout when ready.
// Build with `--features registers` so it serves FC03/FC06/FC16.
func startTestServer(t *testing.T) (port uint16, stop func()) {
	t.Helper()

	// Locate the workspace root (mbus-ffi/go/.. -> mbus-ffi/.. -> root).
	wd, err := os.Getwd()
	if err != nil {
		t.Fatalf("getwd: %v", err)
	}
	root := filepath.Clean(filepath.Join(wd, "..", "..", "..", ".."))

	cmd := exec.Command(
		"cargo", "run", "--release", "-q",
		"-p", "mbus-ffi",
		"--example", "dotnet_test_server",
		"--features", "dotnet,registers,traffic",
	)
	cmd.Dir = root
	stdout, err := cmd.StdoutPipe()
	if err != nil {
		t.Fatalf("stdout pipe: %v", err)
	}
	cmd.Stderr = os.Stderr
	if err := cmd.Start(); err != nil {
		t.Skipf("cannot run cargo (skipping integration test): %v", err)
	}

	// Wait for the LISTENING line.
	scanner := bufio.NewScanner(stdout)
	deadline := time.Now().Add(60 * time.Second)
	for scanner.Scan() {
		line := strings.TrimSpace(scanner.Text())
		if strings.HasPrefix(line, "LISTENING ") {
			p, err := strconv.ParseUint(strings.TrimPrefix(line, "LISTENING "), 10, 16)
			if err != nil {
				_ = cmd.Process.Kill()
				t.Fatalf("parse port from %q: %v", line, err)
			}
			port = uint16(p)
			break
		}
		if time.Now().After(deadline) {
			_ = cmd.Process.Kill()
			t.Fatal("timed out waiting for test server LISTENING line")
		}
	}

	stop = func() {
		_ = cmd.Process.Kill()
		_ = cmd.Wait()
	}
	return port, stop
}

func TestRoundTripReadWriteHoldingRegisters(t *testing.T) {
	port, stop := startTestServer(t)
	defer stop()

	c, err := tcp.NewClient("127.0.0.1", port, tcp.WithTimeout(2*time.Second))
	if err != nil {
		t.Fatal(err)
	}
	defer c.Close()

	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()

	if err := c.Connect(ctx); err != nil {
		t.Fatalf("connect: %v", err)
	}

	// FC06 — write a single register.
	if err := c.WriteSingleRegister(ctx, 1, 10, 0xCAFE); err != nil {
		t.Fatalf("WriteSingleRegister: %v", err)
	}

	// FC03 — read it back.
	got, err := c.ReadHoldingRegisters(ctx, 1, 10, 1)
	if err != nil {
		t.Fatalf("ReadHoldingRegisters: %v", err)
	}
	if len(got) != 1 || got[0] != 0xCAFE {
		t.Fatalf("got %v, want [0xCAFE]", got)
	}

	// FC16 — write multiple, then FC03 read range.
	want := []uint16{1, 2, 3, 4, 5}
	if err := c.WriteMultipleRegisters(ctx, 1, 100, want); err != nil {
		t.Fatalf("WriteMultipleRegisters: %v", err)
	}
	got2, err := c.ReadHoldingRegisters(ctx, 1, 100, uint16(len(want)))
	if err != nil {
		t.Fatalf("ReadHoldingRegisters: %v", err)
	}
	if len(got2) != len(want) {
		t.Fatalf("len mismatch: got %d, want %d", len(got2), len(want))
	}
	for i := range want {
		if got2[i] != want[i] {
			t.Fatalf("idx %d: got %d, want %d", i, got2[i], want[i])
		}
	}
}
