//go:build cgo

package cgo

/*
#include "modbus_rs_go.h"

// The Go runtime manages its own threads and provides highly concurrent
// scheduling. For the Modbus C FFI, the underlying Rust code uses `mbus_pool_lock`
// and related hooks to ensure thread safety of the C API pools. Since Go is highly
// concurrent, we define these as no-ops here. The Go bindings use the C API in a
// thread-safe manner (e.g. they don't concurrently delete and access the same client
// without synchronization on the Go side, and the underlying Rust library is thread-safe).

void mbus_pool_lock(void) {}
void mbus_pool_unlock(void) {}

void mbus_client_lock(MbusClientId id) { (void)id; }
void mbus_client_unlock(MbusClientId id) { (void)id; }

void mbus_gateway_lock(MbusGatewayId id) { (void)id; }
void mbus_gateway_unlock(MbusGatewayId id) { (void)id; }
*/
import "C"
