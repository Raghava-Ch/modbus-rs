# modbus-server

`modbus-server` contains the server-side foundations for compile-time mapping.

## Status

Phase 1 is focused on derive-driven model declarations for:
- coils
- holding registers
- input registers

The stack remains the owner of protocol memory. User code declares typed models and derives mapping metadata.

## Phase 1 API

`mbus-server` re-exports these derives:
- `CoilsModel`
- `HoldingRegistersModel`
- `InputRegistersModel`
- `modbus_app`

### Coils example

```rust
use mbus_server::CoilsModel;

#[derive(Debug, Clone, Default, CoilsModel)]
struct Coils {
	#[coil(addr = 0)]
	run_enable: bool,
	#[coil(addr = 1)]
	fault_reset: bool,
}
```

### Holding registers + app routing example

```rust
use mbus_server::{HoldingRegistersModel, modbus_app};

#[derive(Debug, Clone, Default, HoldingRegistersModel)]
struct ChillerRegs {
	#[reg(addr = 0, scale = 0.1, unit = "C")]
	supply_temp: u16,
	#[reg(addr = 1)]
	return_temp: u16,
}

#[derive(Debug, Default)]
#[modbus_app(holding_registers(chiller))]
struct App {
	chiller: ChillerRegs,
}
```

### Generated API

`CoilsModel` generates:
- `CoilMap` implementation with FC01/FC05/FC15 support

`HoldingRegistersModel` generates:
- per-field getter methods (`field_name()`)
- per-field setter methods (`set_field_name(u16)`)
- `HoldingRegisterMap` implementation with FC03 `encode()` support
- optional engineering helpers when `scale` is provided:
	- `field_name_scaled() -> f32`
	- `set_field_name_scaled(f32) -> Result<(), MbusError>`
- optional unit helper when `unit` is provided:
	- `field_name_unit() -> &'static str`

`InputRegistersModel` generates:
- per-field getter methods (`field_name()`)
- per-field setter methods (`set_field_name(u16)`) for local model updates
- `InputRegisterMap` implementation with FC04 `encode()` support only
- no register write trait methods (no `write_single` / `write_many`)

### Ergonomic encode() calls

To call `encode()` as a method (`my_regs.encode(...)`) without manually importing
the trait path each time, import the crate prelude:

```rust
use mbus_server::prelude::*;
```

`modbus_app` generates:
- direct `ModbusAppHandler` implementation on your app struct
- compile-time overlap checks across all selected `holding_registers(...)` maps

### Write Hooks

`modbus_app` now also supports pre-write approval hooks for server-side writes.

Field-level opt-in for single-write fallback to the batch hook:

```rust
#[derive(Debug, Default, CoilsModel)]
struct Coils {
	#[coil(addr = 0)]
	run_enable: bool,
	#[coil(addr = 1, notify_via_batch = true)]
	alarm_ack: bool,
}

#[derive(Debug, Default, HoldingRegistersModel)]
struct Holding {
	#[reg(addr = 10)]
	setpoint: u16,
	#[reg(addr = 11, notify_via_batch = true)]
	fan_speed: u16,
}
```

Hook wiring on the app struct:

```rust
#[derive(Debug, Default)]
#[modbus_app(
	coils(coils, on_batch_write = on_coil_batch, on_write_0 = on_run_enable),
	holding_registers(regs, on_batch_write = on_reg_batch, on_write_10 = on_setpoint),
)]
struct App {
	coils: Coils,
	regs: Holding,
}
```

Hook signatures:

- single coil: `fn hook(&mut self, address: u16, old: bool, new: bool) -> Result<(), MbusError>`
- single register: `fn hook(&mut self, address: u16, old: u16, new: u16) -> Result<(), MbusError>`
- batch coil: `fn hook(&mut self, start: u16, qty: u16, values: &[u8]) -> Result<(), MbusError>`
- batch register: `fn hook(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError>`

Behavior:

- `on_write_N` runs before a single FC05 / FC06 write commits.
- `on_batch_write` runs before an FC0F / FC10 write commits.
- if a field has `notify_via_batch = true`, a single write with no `on_write_N` hook will call the batch hook with `qty = 1`.
- returning `Err(...)` rejects the write and leaves the model unchanged.

Validation errors (compile time):

- `notify_via_batch` used on a field but no `on_batch_write` configured for that group:
	add `on_batch_write = my_hook` to the matching `coils(...)` or `holding_registers(...)` group.
- duplicate `on_write_N` in the same group:
	keep only one hook binding per address.
- `on_write_N` targets an address outside selected map ranges:
	use an address covered by the selected maps or update `#[modbus_app(...)]` map selection.
- configured hook signature does not match expected form:
	ensure signatures are exactly:
	single coil `fn(&mut self, address: u16, old: bool, new: bool) -> Result<(), MbusError>`
	single register `fn(&mut self, address: u16, old: u16, new: u16) -> Result<(), MbusError>`
	batch coil `fn(&mut self, start: u16, qty: u16, values: &[u8]) -> Result<(), MbusError>`
	batch register `fn(&mut self, start: u16, qty: u16, values: &[u16]) -> Result<(), MbusError>`

See `examples/write_hooks.rs` for a runnable end-to-end example.

### Forwarding wrapper for runtime-owned app state

`modbus_app` implements `ModbusAppHandler` on your concrete app model. In real deployments,
that model is often wrapped by runtime state containers (mutexes, critical sections,
RTOS primitives, shared ownership handles, etc.).

To avoid writing repetitive callback delegation, `mbus-server` provides:
- `ModbusAppAccess`: one method (`with_app_mut`) to provide temporary mutable access
- `ForwardingApp<A>`: adapts any `A: ModbusAppAccess` into `ModbusAppHandler`

This design keeps `mbus-server` `no_std` while letting user code choose the synchronization
mechanism that matches the execution environment.

#### `std` host/server usage (desktop, Linux, macOS, Windows)

```rust
use std::sync::{Arc, Mutex};
use mbus_server::{ForwardingApp, ModbusAppAccess, ModbusAppHandler};

#[derive(Clone)]
struct SharedApp {
	inner: Arc<Mutex<MyApp>>,
}

impl ModbusAppAccess for SharedApp {
	type App = MyApp;

	fn with_app_mut<R, F>(&self, f: F) -> R
	where
		F: FnOnce(&mut Self::App) -> R,
	{
		let mut guard = self.inner.lock().expect("app lock poisoned");
		f(&mut guard)
	}
}

let app = ForwardingApp::new(shared_state);
// pass `app` to ServerServices::new(...)
```

#### Bare-metal / RTOS usage

Implement `ModbusAppAccess` with your own primitive:
- bare-metal single thread: interior mutability + ownership discipline
- interrupt-safe: critical-section lock
- RTOS: mutex/semaphore wrappers

The protocol stack only depends on `with_app_mut`, not on any concrete lock type.

### Attribute keys (phase 1)

For `CoilsModel`:
- `addr` required

For `HoldingRegistersModel`:
- `addr` required via `#[reg(addr = N)]`
- field type must be `u16` (wire-ready register word)
- `scale` optional numeric literal (generates `*_scaled` helper methods)
- `unit` optional string literal (generates `*_unit` helper method)

For `InputRegistersModel`:
- `addr` required via `#[reg(addr = N)]`
- field type must be `u16` (wire-ready register word)
- `scale` optional numeric literal (generates `*_scaled` helper methods)
- `unit` optional string literal (generates `*_unit` helper method)

## Feature gates

- `server`: enables server-only runtime extensions such as opt-in Serial
	broadcast write handling (enabled by default in `mbus-server` itself)
- `holding-registers`: enables FC03/FC06/FC10 server handling and `HoldingRegistersModel`
- `input-registers`: enables FC04 server handling and `InputRegistersModel`
- `registers`: compatibility alias that enables both `holding-registers` and `input-registers`

Compile-time diagnostics distinguish between:
- duplicate register addresses
- overlapping map ranges in `#[modbus_app]`

## Current Behavior

- Compile-time descriptor tables are generated by derive macros.
- Validation errors are emitted at compile time for common mapping mistakes.
- Runtime arrays remain stack-owned.

## Resilience configuration

`ServerServices` accepts a `ResilienceConfig` that controls queueing, retries,
and timeout policy.

### Timed retry schedule

- `max_send_retries`: retry budget for queued responses after send failure.
- `timeouts.response_retry_interval_ms`: minimum delay between retry attempts
	for the same queued response.
- `clock_fn`: required to enforce the retry interval deterministically.

If `response_retry_interval_ms > 0` and `clock_fn` is set, retries are
time-gated by elapsed milliseconds. If no clock is provided, retries are still
performed but cadence is poll-driven.

### Overflow policy

- `timeouts.overflow_policy = DropResponse`: legacy behavior. If a response send
	fails and the retry queue is full, the response is dropped.
- `timeouts.overflow_policy = RejectRequest`: when the response retry queue reaches
	80% utilization, new addressed unicast requests are rejected before dispatch so
	the server avoids applying more state changes that it may not be able to confirm.
	The rejection is sent as a normal Modbus exception response using the current
	`TooManyRequests -> ServerDeviceFailure` mapping.

Important protocol note:
- broadcast frames and misaddressed frames are never answered, even under
	back-pressure.
- when `enable_broadcast_writes = true`, supported Serial broadcast writes are
	processed immediately with no response and no interaction with the response queue.
- TCP broadcast remains silently discarded even when broadcast writes are enabled.
- `RejectRequest` therefore only applies to requests actually addressed to this
	server.

### Example

```rust
use mbus_server::{OverflowPolicy, ResilienceConfig, TimeoutConfig};

let resilience = ResilienceConfig {
		timeouts: TimeoutConfig {
				app_callback_ms: 20,
				send_ms: 50,
				response_retry_interval_ms: 100,
				request_deadline_ms: 500,
				strict_mode: true,
				overflow_policy: OverflowPolicy::RejectRequest,
		},
		clock_fn: Some(my_monotonic_ms),
		max_send_retries: 3,
		enable_priority_queue: true,
		enable_broadcast_writes: true,
};
```

This configuration means:
- app callbacks are monitored against a 20ms threshold
- send duration is monitored against a 50ms threshold
- failed response sends are retried at most every 100ms
- queued requests older than 500ms are expired using strict mode behaviour
- once the response retry queue reaches 80% utilization, new addressed unicast
	requests are rejected instead of admitting more work that may not be confirmable
- supported Serial broadcast writes bypass response queue pressure entirely and
	are applied without any reply

## Detailed design

See [documentation/server_macro_phase1.md](../documentation/server_macro_phase1.md).

## License

Licensed under the repository root `LICENSE`.
