# Server Policies

Timeout, retry, queue, and broadcast behavior controlled by `ResilienceConfig`.

---

## Overview

`ServerServices` accepts a `ResilienceConfig` at construction time:

```rust
use modbus_rs::{OverflowPolicy, ResilienceConfig, TimeoutConfig};

fn my_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

let resilience = ResilienceConfig {
    timeouts: TimeoutConfig {
        app_callback_ms: 20,
        send_ms: 50,
        response_retry_interval_ms: 100,
        request_deadline_ms: 500,
        strict_mode: false,
        overflow_policy: OverflowPolicy::RejectRequest,
    },
    clock_fn: Some(my_clock_ms),
    max_send_retries: 3,
    enable_priority_queue: true,
    enable_broadcast_writes: false,
};
```

All resilience features are off by default unless you opt in through the struct fields.

---

## Field Reference

### `ResilienceConfig`

| Field | Default | Meaning |
|-------|---------|---------|
| `timeouts` | `TimeoutConfig::default()` | Per-phase thresholds and overflow policy |
| `clock_fn` | `None` | Monotonic millisecond clock for all time-based checks |
| `max_send_retries` | `3` | Retry budget for failed response sends |
| `enable_priority_queue` | `false` | Queue and prioritize requests instead of dispatching immediately |
| `enable_broadcast_writes` | `false` | Accept serial broadcast write requests |

### `TimeoutConfig`

| Field | Default | Meaning |
|-------|---------|---------|
| `app_callback_ms` | `0` | Log when a callback exceeds this duration |
| `send_ms` | `0` | Log when a send exceeds this duration |
| `response_retry_interval_ms` | `0` | Minimum delay between queued response retry attempts |
| `request_deadline_ms` | `0` | Maximum age for queued requests |
| `strict_mode` | `false` | Try to emit an exception before dropping stale queued requests |
| `overflow_policy` | `DropResponse` | Behavior when the response retry queue is under pressure |

A `clock_fn` is required for the time-based thresholds to have any effect.

---

## Overflow Policy

### `DropResponse`

Legacy behavior. If the response retry queue is full, the failed response is dropped.

### `RejectRequest`

When the response queue reaches 80% utilization, addressed unicast requests are rejected before
they are allowed to mutate state. This reduces asymmetric-state failures when a response cannot be delivered.

Broadcast frames and misaddressed frames are still discarded silently.

---

## Priority Queue

When `enable_priority_queue = true`, requests are classified by `RequestPriority`:

| Priority | Function Codes |
|----------|----------------|
| `Maintenance` | FC08, FC0B, FC0C, FC11, FC2B |
| `Write` | FC05, FC06, FC0F, FC10, FC15, FC16, FC17 |
| `Read` | FC01, FC02, FC03, FC04, FC14, FC18 |
| `Other` | Anything not covered above |

When disabled, requests are dispatched as soon as they are received.

---

## Broadcast Writes

Broadcast writes are serial-only and require `enable_broadcast_writes = true`.

- use `UnitIdOrSlaveAddr::new_broadcast_address()` on the sending side
- the server callback sees `uid.is_broadcast() == true`
- no response is sent
- TCP unit id `0` is not treated as a writable broadcast

---

## Queue Depth

`ServerServices::new(...)` uses the default queue depth of `8`.

If you want a different depth, instantiate the server with an explicit const generic and
call `with_queue_depth(...)`:

```rust
let mut server: modbus_rs::ServerServices<_, _, 16> =
    modbus_rs::ServerServices::with_queue_depth(
        transport,
        app,
        config,
        unit_id,
        resilience,
    );
```

This depth controls both the buffered request queue and the pending response queue.

---

## See Also

- [Building Applications](building_applications.md)
- [Architecture](architecture.md)
- [Function Codes](function_codes.md)
